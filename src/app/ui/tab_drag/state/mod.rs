mod autoscroll;
mod drag;
mod drop_target;

use eframe::egui;

pub(super) const TAB_DRAG_THRESHOLD: f32 = 8.0;

#[derive(Clone)]
pub(super) struct TabDragState {
    pub(super) source_index: usize,
    pub(super) dragged_indices: Vec<usize>,
    pub(super) start_pos: egui::Pos2,
    pub(super) current_pos: egui::Pos2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabDropAxis {
    Horizontal,
    Vertical,
}

pub(crate) struct TabDropZone {
    pub(crate) axis: TabDropAxis,
    pub(crate) entries: Vec<TabRectEntry>,
}

#[derive(Clone, Copy)]
pub(crate) struct TabRectEntry {
    pub(crate) index: usize,
    pub(crate) rect: egui::Rect,
    pub(crate) combine_enabled: bool,
}

pub(crate) use autoscroll::auto_scroll_delta;
pub(crate) use drag::{
    active_drag_sources_for_context, begin_tab_drag_if_needed, is_drag_active_for_context,
};
pub(crate) use drag::{
    clear_tab_drag_state, current_tab_drag_state_for_context, drag_is_active,
    update_current_tab_drag,
};
pub(crate) use drop_target::{TabDropIntent, locate_drop_intent, resolve_drop_slot};
