mod cache;
use crate::app::domain::{BufferState, buffer::HistoryToken};
use eframe::egui;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone)]
struct Row {
    id: u64,
    depth: usize,
    fork: bool,
}
#[derive(Clone)]
struct TreeRows {
    token: HistoryToken,
    rows: Vec<Row>,
    positions: HashMap<u64, usize>,
    structure: (u64, usize, u64),
}

fn rows(buffer: &BufferState) -> Vec<Row> {
    let entries = buffer.document().history_entries();
    let mut children: HashMap<u64, Vec<usize>> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        children.entry(entry.parent_id).or_default().push(index);
    }
    let mut rows = vec![Row {
        id: 0,
        depth: 0,
        fork: false,
    }];
    let mut pending = Vec::new();
    if let Some(roots) = children.get(&0) {
        pending.extend(roots.iter().rev().map(|&i| (i, 0)));
    }
    while let Some((index, depth)) = pending.pop() {
        let entry = &entries[index];
        let fork = children.get(&entry.parent_id).is_some_and(|c| c.len() > 1);
        let depth = depth + usize::from(fork);
        rows.push(Row {
            id: entry.id,
            depth,
            fork,
        });
        if let Some(next) = children.get(&entry.id) {
            pending.extend(next.iter().rev().map(|&i| (i, depth)));
        }
    }
    rows
}

pub(super) fn show(ui: &mut egui::Ui, buffer: &BufferState) -> Option<u64> {
    let document = buffer.document();
    let key = egui::Id::unique(("history.tree.rows", buffer.id));
    let (cached, changed) = cache::load(ui.ctx(), buffer, key);
    let collapsed_key = key.with("collapsed");
    let mut collapsed = ui
        .ctx()
        .data(|d| d.get_temp::<HashSet<u64>>(collapsed_key))
        .unwrap_or_default();
    if changed {
        collapsed.retain(|id| !document.is_history_node_applied(*id));
    }
    let visible = cache::visible(ui.ctx(), key, &cached, &collapsed);
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt(("history.tree.scroll", buffer.id))
        .max_height(360.0)
        .auto_shrink([false, true]);
    if changed {
        let index = visible
            .as_ref()
            .and_then(|indices| {
                indices
                    .iter()
                    .position(|&i| cached.rows[i].id == document.history_now())
            })
            .or_else(|| {
                if visible.is_none() {
                    cached.positions.get(&document.history_now()).copied()
                } else {
                    None
                }
            });
        if let Some(index) = index {
            scroll = scroll.vertical_scroll_offset((index as f32 * 28.0 - 120.0).max(0.0));
        }
    }
    let mut action = None;
    scroll.show_rows(
        ui,
        28.0,
        visible
            .as_ref()
            .map_or(cached.rows.len(), |indices| indices.len()),
        |ui, range| {
            for index in range {
                let row = &cached.rows[visible.as_ref().map_or(index, |indices| indices[index])];
                if show_row(ui, buffer, row, &mut collapsed) {
                    action = Some(row.id);
                }
            }
        },
    );
    ui.ctx()
        .data_mut(|d| d.insert_temp(collapsed_key, collapsed));
    ui.ctx().data_mut(|d| d.insert_temp(key, Some(cached)));
    action
}

fn show_row(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    row: &Row,
    collapsed: &mut HashSet<u64>,
) -> bool {
    let document = buffer.document();
    let mut clicked = false;
    let title = if row.id == 0 {
        "Empty document"
    } else {
        let Ok(i) = document
            .history_entries()
            .binary_search_by_key(&row.id, |entry| entry.id)
        else {
            return false;
        };
        &document.history_entries()[i].summary
    };
    let current = row.id == document.history_now();
    crate::app::ui::widget_ids::scope(ui, (buffer.id, row.id), |ui| {
        ui.horizontal(|ui| {
            ui.add_space((row.depth as f32 * 14.0).min(140.0));
            if row.fork {
                if ui
                    .small_button(if collapsed.contains(&row.id) {
                        "+"
                    } else {
                        "−"
                    })
                    .on_hover_text("Expand or collapse branch")
                    .clicked()
                    && !collapsed.remove(&row.id)
                {
                    collapsed.insert(row.id);
                }
                ui.weak(format!("Branch {}", row.id));
            }
            let prefix = if current { "Now · " } else { "" };
            let label = format!("{prefix}{title}");
            let text = egui::RichText::new(label.clone());
            let text = if current {
                text.strong()
            } else if row.id == 0 || document.is_on_history_route(row.id) {
                text
            } else {
                text.weak()
            };
            let response = ui
                .selectable_label(current, text)
                .on_hover_text(format!("State {} · {title}", row.id));
            if response.clicked() {
                clicked = true;
            }
            if row.id == document.history_tip() {
                ui.weak("Selected tip");
            }
        });
    });
    clicked
}
