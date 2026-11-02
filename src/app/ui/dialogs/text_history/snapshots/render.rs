use super::super::{model::HistoryAction, states::States};
use super::Catalog;
use crate::app::{
    theme::{tab_selected_accent, text_muted},
    ui::{recovery::timestamp, widget_ids},
};
use eframe::egui;

pub(in super::super) fn covered_positions(
    catalog: &Catalog,
    id: u64,
    states: &States<'_>,
) -> Vec<(usize, String)> {
    let Some(file) = catalog.get(&id) else {
        return Vec::new();
    };
    file.rows
        .iter()
        .filter_map(|row| {
            let boundary = row.covered?;
            let position = match boundary.entry_id {
                Some(id) => {
                    states.entries.iter().position(|e| e.id == id)? + usize::from(!boundary.before)
                }
                None => 0,
            };
            Some((position, timestamp(row.point.created_at_unix_seconds)))
        })
        .collect()
}

pub(in super::super) fn older_rows(
    ui: &mut egui::Ui,
    id: u64,
    catalog: &Catalog,
    action: &mut Option<HistoryAction>,
    hover: bool,
) -> bool {
    let Some(file) = catalog.get(&id) else {
        return false;
    };
    let mut hovered = false;
    for row in file.rows.iter().filter(|row| row.covered.is_none()) {
        widget_ids::scope(ui, ("history.snapshot", id, row.point.id), |ui| {
            ui.horizontal(|ui| {
                let selected = file.selected == Some(row.point.id);
                let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(20.0, 26.0));
                diamond(ui, rect.center(), selected);
                let label = timestamp(row.point.created_at_unix_seconds);
                let response = ui
                    .add_sized(
                        egui::vec2(ui.available_width(), 26.0),
                        egui::Button::new(&label).selected(selected).frame(false),
                    )
                    .on_hover_text(format!(
                        "{} · {} bytes{}\nHover or click to visit · Esc to close",
                        file.label,
                        row.point.bytes,
                        if row.point.has_history {
                            " · saved branch"
                        } else {
                            ""
                        }
                    ));
                hovered |= hover && response.hovered();
                let target = HistoryAction::Snapshot {
                    buffer_id: id,
                    version_id: row.point.id,
                };
                let hover_changed = hover
                    && response.hovered()
                    && super::super::rail::hover_changed(ui.ctx(), target);
                if response.clicked() || hover_changed {
                    *action = Some(target);
                }
            });
        });
    }
    hovered
}

pub(in super::super) fn diamond(ui: &egui::Ui, center: egui::Pos2, selected: bool) {
    let points = vec![
        center + egui::vec2(0.0, -5.0),
        center + egui::vec2(5.0, 0.0),
        center + egui::vec2(0.0, 5.0),
        center + egui::vec2(-5.0, 0.0),
    ];
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        if selected {
            tab_selected_accent(ui)
        } else {
            ui.visuals().window_fill()
        },
        egui::Stroke::new(1.0, tab_selected_accent(ui)),
    ));
}

pub(in super::super) fn footer(ui: &mut egui::Ui, catalog: &Catalog) {
    let status = ui
        .ctx()
        .data(|d| d.get_temp::<String>(super::visit::status_key()))
        .or_else(|| catalog.values().find_map(|f| f.error.clone()));
    if let Some(status) = status {
        let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), 20.0));
        ui.put(
            rect,
            egui::Label::new(egui::RichText::new(status).small().color(text_muted(ui))).truncate(),
        );
    }
}
