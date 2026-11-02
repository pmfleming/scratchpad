use super::{
    model::{HistoryAction, TextHistoryRow, row_from_entry, timeline_rows_from_entries},
    snapshots::{self, Catalog},
    states::States,
};
use crate::app::{
    text_history::TextHistoryEntryView,
    theme,
    ui::{callout, widget_ids},
};
use eframe::egui;

pub(super) fn focus_id() -> egui::Id {
    egui::Id::unique("history.rail.focus")
}
fn hover_id() -> egui::Id {
    egui::Id::unique("history.rail.hover")
}

pub(super) fn render(
    ui: &mut egui::Ui,
    entries: &[TextHistoryEntryView],
    catalog: &Catalog,
    target: Option<u64>,
    ids: &[u64],
    action: &mut Option<HistoryAction>,
) {
    let mut focus_requested = false;
    let previous_hover = ui
        .ctx()
        .data_mut(|d| d.remove_temp::<(u64, usize)>(hover_id()));
    let scroll = egui::ScrollArea::vertical()
        .id_salt(("history.states", target))
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 20.0 - ui.spacing().item_spacing.y).max(64.0))
        .min_scrolled_height(64.0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(id) = target {
                let file = FileStates::new(entries, catalog, id);
                for k in (0..=file.states.entries.len()).rev() {
                    let row = k
                        .checked_sub(1)
                        .map(|i| row_from_entry(file.states.entries[i]));
                    focus_requested |=
                        file.render_state(ui, k, row.as_ref(), None, previous_hover, action);
                }
            } else {
                // Build each file's path once, not once per timeline row.
                let paths: std::collections::HashMap<_, _> = ids
                    .iter()
                    .map(|&id| (id, FileStates::new(entries, catalog, id)))
                    .collect();
                let positions: std::collections::HashMap<_, _> = paths
                    .iter()
                    .flat_map(|(&id, file)| {
                        file.states
                            .entries
                            .iter()
                            .enumerate()
                            .map(move |(i, entry)| ((id, entry.id), i + 1))
                    })
                    .collect();
                let rows = timeline_rows_from_entries(entries.iter());
                for row in rows {
                    let Some(file) = paths.get(&row.buffer_id) else {
                        continue;
                    };
                    let k = positions[&(row.buffer_id, row.entry_id)];
                    let name = file.states.entries.first().map(|e| short_name(&e.label));
                    focus_requested |=
                        file.render_state(ui, k, Some(&row), name, previous_hover, action);
                }
                // A file at position zero still needs its own current-state marker.
                for &id in ids {
                    let file = &paths[&id];
                    let name = catalog.get(&id).map(|f| f.label.as_str());
                    focus_requested |= file.render_state(ui, 0, None, name, previous_hover, action);
                }
            }
            for &id in ids {
                if catalog.get(&id).is_some_and(|f| f.older_change_count() > 0) {
                    ui.add_space(8.0);
                    ui.separator();
                    if target.is_none()
                        && let Some(file) = catalog.get(&id)
                    {
                        ui.weak(&file.label);
                    }
                    snapshots::older_rows(ui, id, catalog, action);
                }
            }
        });
    focus_rail(ui, scroll.inner_rect, focus_requested);
}

fn focus_rail(ui: &mut egui::Ui, rect: egui::Rect, focus_requested: bool) {
    let response = widget_ids::interact(
        ui,
        rect,
        focus_id(),
        egui::Sense::focusable_noninteractive(),
        "history.rail",
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ListBox, true, "History states"));
    if focus_requested {
        response.request_focus();
    }
    if response.has_focus() {
        super::lock_navigation_focus(ui.ctx(), focus_id());
        ui.painter().rect_stroke(
            rect,
            0,
            egui::Stroke::new(1.0, theme::tab_selected_accent(ui)),
            egui::StrokeKind::Inside,
        );
    }
}

fn short_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

struct FileStates<'a> {
    id: u64,
    states: States<'a>,
    markers: Vec<(usize, String)>,
    visiting: bool,
}

impl<'a> FileStates<'a> {
    fn new(entries: &'a [TextHistoryEntryView], catalog: &Catalog, id: u64) -> Self {
        let states = States::new(entries, id);
        Self {
            id,
            markers: snapshots::covered_positions(catalog, id, &states),
            visiting: catalog.get(&id).is_some_and(|f| f.selected.is_some()),
            states,
        }
    }

    fn render_state(
        &self,
        ui: &mut egui::Ui,
        k: usize,
        row: Option<&TextHistoryRow<'_>>,
        name: Option<&str>,
        hovered: Option<(u64, usize)>,
        action: &mut Option<HistoryAction>,
    ) -> bool {
        let id = self.id;
        let states = &self.states;
        let visiting = self.visiting;
        let current = !visiting && k == states.position;
        let reachable = states.reachable(k);
        let undone = k > states.position;
        let label = row.map_or("Oldest retained state", |r| r.title);
        let hint = if !reachable {
            "Cannot replay across this change".to_owned()
        } else if visiting {
            "Return to this state".into()
        } else if current {
            "Current state".into()
        } else {
            format!(
                "{} {}",
                if k < states.position { "Undo" } else { "Redo" },
                k.abs_diff(states.position)
            )
        };
        let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), 34.0));
        if current {
            reveal_changed_state(ui, rect, id, k);
        }
        if !ui.is_rect_visible(rect) {
            return false;
        }
        let response = widget_ids::interact(
            ui,
            rect,
            widget_ids::local(ui, ("state", id, k)),
            egui::Sense::click(),
            "history.state",
        )
        .on_hover_text(format!(
            "{label}\n{hint}{}",
            row.map_or(String::new(), |r| format!("\n{} · {}", r.label, r.source))
        ));
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                reachable,
                current,
                format!("State: {label}"),
            )
        });
        if response.hovered() && reachable {
            ui.ctx().data_mut(|d| d.insert_temp(hover_id(), (id, k)));
        }
        let affected = hovered.is_some_and(|(buffer, destination)| {
            buffer == id
                && states.reachable(destination)
                && k > destination.min(states.position)
                && k <= destination.max(states.position)
        });
        if current || affected || response.hovered() {
            ui.painter().rect_filled(
                rect,
                0,
                if current || affected {
                    theme::tab_selected_bg(ui)
                } else {
                    theme::action_bg(ui)
                },
            );
        }
        paint_node(
            ui,
            rect,
            (id, k),
            current,
            undone,
            self.markers
                .iter()
                .find(|(position, _)| *position == k)
                .map(|(_, time)| time.as_str()),
        );
        let icon = if reachable {
            row.map_or(egui_phosphor::regular::FILE_DASHED, |r| r.icon)
        } else {
            egui_phosphor::regular::LOCK_SIMPLE
        };
        ui.painter().text(
            egui::pos2(rect.left() + 32.0, rect.center().y),
            egui::Align2::CENTER_CENTER,
            icon,
            egui_phosphor::font_id(15.0),
            callout::muted_text(ui),
        );
        paint_label(ui, rect, row, name);
        if response.clicked() && reachable && (!current || visiting) {
            *action = Some(HistoryAction::State {
                buffer_id: id,
                after: states.after(k),
            });
        }
        response.clicked()
    }
}

fn reveal_changed_state(ui: &mut egui::Ui, rect: egui::Rect, buffer: u64, position: usize) {
    let key = widget_ids::local(ui, ("history.current", buffer));
    let previous = ui.ctx().data_mut(|d| {
        let previous = d.get_temp::<usize>(key);
        d.insert_temp(key, position);
        previous
    });
    if previous != Some(position) && ui.ctx().memory(|m| m.has_focus(focus_id())) {
        ui.scroll_to_rect(rect, Some(egui::Align::Center));
    }
}

fn paint_label(
    ui: &egui::Ui,
    rect: egui::Rect,
    row: Option<&TextHistoryRow<'_>>,
    name: Option<&str>,
) {
    let mut job = egui::text::LayoutJob::default();
    if let Some(name) = name {
        append(
            &mut job,
            &format!("{}  ·  ", short_name(name)),
            callout::muted_text(ui),
            false,
        );
    }
    if let Some(row) = row {
        append_diff(ui, &mut job, row);
    } else {
        append(
            &mut job,
            "Oldest retained state",
            callout::muted_text(ui),
            false,
        );
    }
    job.wrap.max_width = (rect.width() - 62.0).max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(
        egui::pos2(rect.left() + 48.0, rect.center().y - galley.size().y / 2.0),
        galley,
        callout::text(ui),
    );
}

fn paint_node(
    ui: &egui::Ui,
    rect: egui::Rect,
    key: (u64, usize),
    current: bool,
    undone: bool,
    marker: Option<&str>,
) {
    let center = egui::pos2(rect.left() + 12.0, rect.center().y);
    let stroke = egui::Stroke::new(1.0, theme::border(ui));
    if undone {
        for offset in [0.0, 7.0, 14.0, 21.0, 28.0] {
            ui.painter().line_segment(
                [
                    egui::pos2(center.x, rect.top() + offset),
                    egui::pos2(center.x, (rect.top() + offset + 3.0).min(rect.bottom())),
                ],
                stroke,
            );
        }
    } else {
        ui.painter().line_segment(
            [
                egui::pos2(center.x, rect.top()),
                egui::pos2(center.x, rect.bottom()),
            ],
            stroke,
        );
    }
    let color = if current {
        theme::tab_selected_accent(ui)
    } else {
        callout::muted_text(ui)
    };
    ui.painter().circle(
        center,
        if current { 5.0 } else { 3.5 },
        if undone {
            ui.visuals().panel_fill
        } else {
            color
        },
        egui::Stroke::new(1.0, color),
    );
    if let Some(time) = marker {
        snapshots::diamond(ui, center, current);
        widget_ids::interact(
            ui,
            egui::Rect::from_center_size(center, egui::vec2(18.0, 24.0)),
            widget_ids::local(ui, ("snapshot.marker", key)),
            egui::Sense::hover(),
            "history.snapshot.marker",
        )
        .on_hover_text(format!("Snapshot · {time}"));
    }
}

fn append(job: &mut egui::text::LayoutJob, text: &str, color: egui::Color32, deleted: bool) {
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(13.0),
            color,
            strikethrough: if deleted {
                egui::Stroke::new(1.0, color)
            } else {
                egui::Stroke::NONE
            },
            ..Default::default()
        },
    );
}

pub(super) fn append_diff(
    ui: &egui::Ui,
    job: &mut egui::text::LayoutJob,
    row: &TextHistoryRow<'_>,
) {
    if row.deleted.is_empty() && row.inserted.is_empty() {
        append(job, row.title, callout::text(ui), false);
        return;
    }
    append(
        job,
        row.deleted,
        super::dim_if(row.undone, theme::diff_del(ui)),
        true,
    );
    if !row.deleted.is_empty() && !row.inserted.is_empty() {
        append(job, " → ", callout::muted_text(ui), false);
    }
    append(
        job,
        row.inserted,
        super::dim_if(row.undone, theme::diff_add(ui)),
        false,
    );
    if row.edit_count > 1 {
        append(
            job,
            &format!("  +{} ranges", row.edit_count - 1),
            callout::muted_text(ui),
            false,
        );
    }
}
