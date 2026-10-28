use super::super::model::HistoryAction;
use super::{Catalog, coverage::Boundary};
use crate::app::{
    theme::{border, text_muted},
    ui::recovery::timestamp,
};
use eframe::egui;

pub(in super::super) fn covered_markers(
    ui: &mut egui::Ui,
    id: u64,
    entry_id: Option<u64>,
    before: bool,
    catalog: &Catalog,
) {
    let Some(file) = catalog.get(&id) else {
        return;
    };
    for row in &file.rows {
        if row.covered == Some(Boundary { entry_id, before }) {
            marker(
                ui,
                &format!(
                    "Snapshot · {}",
                    timestamp(row.point.created_at_unix_seconds)
                ),
            )
            .on_hover_text("This state is already reachable through undo/redo.");
        }
    }
}

pub(in super::super) fn older_rows(
    ui: &mut egui::Ui,
    id: u64,
    catalog: &Catalog,
    action: &mut Option<HistoryAction>,
) {
    let Some(file) = catalog.get(&id) else {
        return;
    };
    if file.loading {
        ui.weak("Loading snapshots…");
    }
    if let Some(error) = &file.error {
        ui.weak("Snapshots unavailable").on_hover_text(error);
    }
    let older: Vec<_> = file
        .rows
        .iter()
        .filter(|row| row.covered.is_none())
        .collect();
    if older.is_empty() {
        return;
    }
    marker(ui, "Snapshots beyond undo");
    for row in older {
        crate::app::ui::widget_ids::scope(ui, ("history.snapshot", id, row.point.id), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE)
                        .font(egui_phosphor::font_id(14.0))
                        .color(text_muted(ui)),
                );
                let selected = file.selected == Some(row.point.id);
                let label = format!(
                    "{}{}",
                    if row.point.has_history {
                        "Saved branch · "
                    } else {
                        ""
                    },
                    timestamp(row.point.created_at_unix_seconds)
                );
                if ui
                    .selectable_label(selected, label)
                    .on_hover_text(format!(
                        "{} bytes · Show this state in the editor",
                        row.point.bytes
                    ))
                    .clicked()
                {
                    *action = Some(HistoryAction::Snapshot {
                        buffer_id: id,
                        version_id: row.point.id,
                    });
                }
                if selected {
                    ui.weak("Now");
                }
            });
        });
    }
}

fn marker(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.horizontal(|ui| {
        let response = ui.label(egui::RichText::new(label).size(11.5).color(text_muted(ui)));
        let rect = crate::app::ui::widget_ids::allocate_exact_rect(
            ui,
            egui::vec2(ui.available_width().max(0.0), 14.0),
        );
        let mut x = rect.left();
        while x < rect.right() {
            ui.painter().line_segment(
                [
                    egui::pos2(x, rect.center().y),
                    egui::pos2((x + 3.0).min(rect.right()), rect.center().y),
                ],
                egui::Stroke::new(1.0, border(ui)),
            );
            x += 7.0;
        }
        response
    })
    .inner
}
