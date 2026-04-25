use crate::app::domain::{BufferId, RenderedLayout};
use crate::app::ui::editor_content::native_editor::CursorRange;
use eframe::egui;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

pub type ViewId = u64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchHighlightState {
    pub ranges: Vec<Range<usize>>,
    pub active_range_index: Option<usize>,
}

#[derive(Clone)]
pub struct EditorViewState {
    pub id: ViewId,
    pub buffer_id: BufferId,
    pub show_line_numbers: bool,
    pub show_control_chars: bool,
    pub editor_has_focus: bool,
    pub latest_layout: Option<RenderedLayout>,
    pub latest_layout_revision: Option<u64>,
    pub cursor_range: Option<CursorRange>,
    pub pending_cursor_range: Option<CursorRange>,
    pub scroll_to_cursor: bool,
    editor_scroll_offset: egui::Vec2,
    pub search_highlights: SearchHighlightState,
}

impl EditorViewState {
    pub fn new(buffer_id: BufferId, show_control_chars: bool) -> Self {
        Self {
            id: next_view_id(),
            buffer_id,
            show_line_numbers: false,
            show_control_chars,
            editor_has_focus: false,
            latest_layout: None,
            latest_layout_revision: None,
            cursor_range: None,
            pending_cursor_range: None,
            scroll_to_cursor: false,
            editor_scroll_offset: egui::Vec2::ZERO,
            search_highlights: SearchHighlightState::default(),
        }
    }

    pub fn restored(
        id: ViewId,
        buffer_id: BufferId,
        show_line_numbers: bool,
        show_control_chars: bool,
    ) -> Self {
        register_existing_view_id(id);
        Self {
            id,
            buffer_id,
            show_line_numbers,
            show_control_chars,
            editor_has_focus: false,
            latest_layout: None,
            latest_layout_revision: None,
            cursor_range: None,
            pending_cursor_range: None,
            scroll_to_cursor: false,
            editor_scroll_offset: egui::Vec2::ZERO,
            search_highlights: SearchHighlightState::default(),
        }
    }

    pub fn editor_scroll_offset(&self) -> egui::Vec2 {
        self.editor_scroll_offset
    }

    pub fn set_editor_scroll_offset(&mut self, offset: egui::Vec2) {
        self.editor_scroll_offset = egui::vec2(
            sanitize_scroll_axis(offset.x),
            sanitize_scroll_axis(offset.y),
        );
    }
}

fn sanitize_scroll_axis(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

pub fn next_view_id() -> ViewId {
    NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed)
}

fn register_existing_view_id(id: ViewId) {
    let next_id = id.saturating_add(1);
    let mut current = NEXT_VIEW_ID.load(Ordering::Relaxed);

    while current < next_id {
        match NEXT_VIEW_ID.compare_exchange(current, next_id, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EditorViewState;
    use eframe::egui;

    #[test]
    fn editor_scroll_offset_is_view_owned_runtime_state() {
        let mut view = EditorViewState::new(7, false);

        assert_eq!(view.editor_scroll_offset(), egui::Vec2::ZERO);

        view.set_editor_scroll_offset(egui::vec2(18.0, 240.0));
        assert_eq!(view.editor_scroll_offset(), egui::vec2(18.0, 240.0));

        view.set_editor_scroll_offset(egui::vec2(-4.0, f32::INFINITY));
        assert_eq!(view.editor_scroll_offset(), egui::Vec2::ZERO);
    }
}
