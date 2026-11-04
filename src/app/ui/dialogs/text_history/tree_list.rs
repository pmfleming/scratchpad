mod cache;
mod graph;
use super::{presentation, style};
use crate::app::{
    domain::{BufferState, buffer::HistoryToken},
    theme,
    ui::widget_ids,
};
use eframe::egui;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Debug)]
struct Row {
    id: u64,
    parent: u64,
    lane: usize,
    fork: bool,
    end: usize,
}
struct TreeRows {
    structure: (u64, usize, u64),
    rows: Vec<Row>,
    positions: HashMap<u64, usize>,
    max_lane: usize,
    forks: HashSet<u64>,
}
#[derive(Clone)]
struct Edge {
    id: u64,
    a: usize,
    b: usize,
    lane: usize,
    parent_lane: usize,
}
struct ViewRows {
    collapsed: HashSet<u64>,
    tree: Arc<TreeRows>,
    indices: Vec<usize>,
    positions: HashMap<u64, usize>,
    edges: Arc<[Edge]>,
}
pub(super) struct Prepared {
    view: Arc<ViewRows>,
    pub changed: bool,
}
impl Prepared {
    pub(super) fn view_id(&self, index: usize) -> u64 {
        self.view.tree.rows[self.view.indices[index]].id
    }
    pub(super) fn len(&self) -> usize {
        self.view.indices.len()
    }
    pub(super) fn now_index(&self, buffer: &BufferState) -> Option<usize> {
        let mut id = buffer.document().history_now();
        loop {
            if let Some(&index) = self.view.positions.get(&id) {
                return Some(index);
            }
            if id == 0 {
                return None;
            }
            id = self.view.tree.rows[*self.view.tree.positions.get(&id)?].parent;
        }
    }
    pub(super) fn now_hidden(&self, buffer: &BufferState) -> bool {
        !self
            .view
            .positions
            .contains_key(&buffer.document().history_now())
    }
}

pub(super) fn reveal_now(ctx: &egui::Context, buffer: &BufferState) {
    let key = cache::key(buffer).with("collapsed");
    ctx.data_mut(|d| {
        if let Some(mut collapsed) = d.get_temp::<HashSet<u64>>(key) {
            collapsed.retain(|id| !buffer.document().is_history_node_applied(*id));
            d.insert_temp(key, collapsed);
        }
    });
}

pub(super) fn prepare(ctx: &egui::Context, buffer: &BufferState) -> Prepared {
    cache::prepare(ctx, buffer)
}

pub(super) fn forks(ctx: &egui::Context, buffer: &BufferState) -> Arc<HashSet<u64>> {
    let tree = cache::tree(ctx, buffer);
    let key = egui::Id::unique(("history.fork_ticks", buffer.id));
    ctx.data_mut(|data| {
        if let Some((structure, forks)) =
            data.get_temp::<((u64, usize, u64), Arc<HashSet<u64>>)>(key)
            && structure == tree.structure
        {
            return forks;
        }
        let forks = Arc::new(tree.forks.clone());
        data.insert_temp(key, (tree.structure, forks.clone()));
        forks
    })
}

pub(super) fn show_row(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    prepared: &Prepared,
    index: usize,
    visible: std::ops::Range<usize>,
) -> bool {
    let view = &prepared.view;
    let row = &view.tree.rows[view.indices[index]];
    let document = buffer.document();
    let current = row.id == document.history_now();
    let change = presentation::change(ui, buffer, row.id);
    let rect =
        widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), style::ROW_HEIGHT));
    let key = egui::Id::unique(("history.change", buffer.id, row.id));
    let label = if current {
        format!("Now · {}", change.title)
    } else {
        change.title.clone()
    };
    let response = widget_ids::interact(ui, rect, key, egui::Sense::click(), "history-row");
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, current, &label)
    });
    if current {
        ui.painter()
            .rect_filled(rect, 0, theme::tab_selected_bg(ui));
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())),
            0,
            theme::tab_selected_accent(ui),
        );
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0, theme::action_bg(ui));
    }
    let gutter = graph::paint(ui, buffer, prepared, index, visible, rect);
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + gutter, rect.top()),
        egui::vec2(20.0, rect.height()),
    );
    style::text(ui, icon_rect, change.icon, 14.0, change.color);
    let color = if current || document.is_history_node_applied(row.id) {
        theme::text_primary(ui)
    } else {
        theme::text_muted(ui)
    };
    let right = metadata(ui, buffer, prepared, index, &response, change.edited_at);
    let (right, toggle) = branch_button(ui, buffer, prepared, index, rect, right);
    change.paint_title(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(icon_rect.right() + 2.0, rect.top()),
            egui::pos2(right, rect.bottom()),
        ),
        color,
    );
    let hover = if change.edited_at == 0 {
        change.title
    } else {
        format!(
            "{}\n{}",
            change.title,
            presentation::full_time(change.edited_at)
        )
    };
    response.on_hover_text(hover).clicked() && !toggle
}

fn metadata(
    ui: &egui::Ui,
    buffer: &BufferState,
    prepared: &Prepared,
    index: usize,
    response: &egui::Response,
    edited_at: u64,
) -> f32 {
    let view = &prepared.view;
    let row = &view.tree.rows[view.indices[index]];
    let document = buffer.document();
    let current = row.id == document.history_now();
    let rect = response.rect;
    let mut right = rect.right() - 8.0;
    let time = presentation::clock(edited_at);
    let previous_time = index.checked_sub(1).map(|i| {
        let id = view.tree.rows[view.indices[i]].id;
        document
            .history_entries()
            .binary_search_by_key(&id, |e| e.id)
            .ok()
            .map_or(0, |i| document.history_entries()[i].edited_at)
    });
    if current {
        let tag = egui::Rect::from_center_size(
            egui::pos2(right - 18.0, rect.center().y),
            egui::vec2(36.0, 17.0),
        );
        ui.painter()
            .rect_filled(tag, 0, theme::tab_selected_accent(ui));
        style::text(
            ui,
            tag.shrink2(egui::vec2(4.0, 0.0)),
            "NOW",
            10.0,
            if ui.visuals().dark_mode {
                egui::Color32::BLACK
            } else {
                egui::Color32::WHITE
            },
        );
        right -= 44.0;
    } else if !time.is_empty() {
        if response.hovered() || previous_time.is_none_or(|at| at / 60 != edited_at / 60) {
            style::text(
                ui,
                egui::Rect::from_min_max(
                    egui::pos2(right - 38.0, rect.top()),
                    egui::pos2(right, rect.bottom()),
                ),
                &time,
                11.0,
                theme::text_muted(ui),
            );
        }
        right -= 44.0;
    }
    if row.id == document.history_tip() && !current {
        let flag = egui::Rect::from_min_size(
            egui::pos2(right - 18.0, rect.top()),
            egui::vec2(18.0, rect.height()),
        );
        style::text(
            ui,
            flag,
            egui_phosphor::regular::FLAG,
            12.0,
            theme::text_muted(ui),
        );
        right -= 20.0;
    }
    right
}

fn branch_button(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    prepared: &Prepared,
    index: usize,
    rect: egui::Rect,
    mut right: f32,
) -> (f32, bool) {
    let view = &prepared.view;
    let row = &view.tree.rows[view.indices[index]];
    let mut toggle = false;
    if row.fork {
        let branch = egui::Rect::from_center_size(
            egui::pos2(right - 22.0, rect.center().y),
            egui::vec2(44.0, 22.0),
        );
        let collapsed = view.collapsed.contains(&row.id);
        let icon = if collapsed {
            egui_phosphor::regular::CARET_RIGHT
        } else {
            egui_phosphor::regular::CARET_DOWN
        };
        let caption = style::icon_label(
            ui,
            icon,
            &format!("{}", row.end - view.tree.positions[&row.id] + 1),
            11.0,
        );
        let button = widget_ids::scope(ui, ("history.branch", buffer.id, row.id), |ui| {
            ui.put(branch, egui::Button::new(caption).frame(collapsed))
        })
        .inner;
        let description = if collapsed {
            "Expand branch"
        } else {
            "Collapse branch"
        };
        button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, description));
        toggle = button.on_hover_text(description).clicked();
        if toggle {
            let mut collapsed = view.collapsed.clone();
            if !collapsed.remove(&row.id) {
                collapsed.insert(row.id);
            }
            ui.ctx()
                .data_mut(|d| d.insert_temp(cache::key(buffer).with("collapsed"), collapsed));
            ui.ctx().memory_mut(|m| m.request_focus(super::focus_id()));
        }
        right -= 50.0;
    }
    (right, toggle)
}
