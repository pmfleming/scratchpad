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
    title: String,
    fork: bool,
}
#[derive(Clone)]
struct TreeRows {
    token: HistoryToken,
    rows: Arc<[Row]>,
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
        title: "Empty document".into(),
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
            title: entry.summary.clone(),
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
    let token = document.history_token();
    let previous = ui.ctx().data(|d| d.get_temp::<TreeRows>(key));
    let changed = previous.as_ref().is_none_or(|c| c.token != token);
    let cached = if changed {
        TreeRows {
            token,
            rows: rows(buffer).into(),
        }
    } else {
        previous.unwrap()
    };
    if changed {
        ui.ctx().data_mut(|d| d.insert_temp(key, cached.clone()));
    }
    let collapsed_key = key.with("collapsed");
    let mut collapsed = ui
        .ctx()
        .data(|d| d.get_temp::<HashSet<u64>>(collapsed_key))
        .unwrap_or_default();
    if changed {
        let route = document.history_route();
        collapsed.retain(|id| !route[..document.operation_undo_depth()].contains(id));
    }
    let mut hidden_depth = None;
    let visible: Vec<_> = cached
        .rows
        .iter()
        .filter(|row| {
            if let Some(depth) = hidden_depth {
                if row.depth > depth || (!row.fork && row.depth == depth) {
                    return false;
                }
                hidden_depth = None;
            }
            if collapsed.contains(&row.id) {
                hidden_depth = Some(row.depth);
            }
            true
        })
        .collect();
    let selected: HashSet<_> = document.history_route().iter().copied().collect();
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt(("history.tree.scroll", buffer.id))
        .max_height(360.0)
        .auto_shrink([false, true]);
    if changed
        && let Some(index) = visible
            .iter()
            .position(|row| row.id == document.history_now())
    {
        scroll = scroll.vertical_scroll_offset((index as f32 * 28.0 - 120.0).max(0.0));
    }
    let mut action = None;
    scroll.show_rows(ui, 28.0, visible.len(), |ui, range| {
        for index in range {
            let row = visible[index];
            let current = row.id == document.history_now();
            ui.push_id((buffer.id, row.id), |ui| {
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
                        {
                            if !collapsed.remove(&row.id) {
                                collapsed.insert(row.id);
                            }
                        }
                        ui.weak(format!("Branch {}", row.id));
                    }
                    let prefix = if current { "Now · " } else { "" };
                    let label = format!("{prefix}{}", row.title);
                    let text = egui::RichText::new(label.clone());
                    let text = if current {
                        text.strong()
                    } else if row.id == 0 || selected.contains(&row.id) {
                        text
                    } else {
                        text.weak()
                    };
                    let response = ui
                        .selectable_label(current, text)
                        .on_hover_text(format!("State {} · {}", row.id, row.title));
                    if response.clicked() {
                        action = Some(row.id);
                    }
                    if row.id == document.history_tip() {
                        ui.weak("Selected tip");
                    }
                });
            });
        }
    });
    ui.ctx()
        .data_mut(|d| d.insert_temp(collapsed_key, collapsed));
    action
}
