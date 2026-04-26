use crate::app::domain::{BufferId, RenderedLayout};
use crate::app::ui::editor_content::native_editor::CursorRange;
use eframe::egui;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

pub type ViewId = u64;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SearchHighlightState {
    pub ranges: Vec<Range<usize>>,
    pub active_range_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PublishedImeOutput {
    rect: egui::Rect,
    cursor_rect: egui::Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorRevealMode {
    KeepVisible,
    Center,
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
    cursor_reveal_mode: Option<CursorRevealMode>,
    editor_scroll_offset: egui::Vec2,
    published_ime_output: Option<PublishedImeOutput>,
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
            cursor_reveal_mode: None,
            editor_scroll_offset: egui::Vec2::ZERO,
            published_ime_output: None,
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
            cursor_reveal_mode: None,
            editor_scroll_offset: egui::Vec2::ZERO,
            published_ime_output: None,
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

    pub fn mark_ime_output(&mut self, rect: egui::Rect, cursor_rect: egui::Rect) -> bool {
        let next = PublishedImeOutput { rect, cursor_rect };
        if self.published_ime_output == Some(next) {
            return false;
        }

        self.published_ime_output = Some(next);
        true
    }

    pub fn clear_ime_output(&mut self) {
        self.published_ime_output = None;
    }

    pub fn request_cursor_reveal(&mut self, mode: CursorRevealMode) {
        self.scroll_to_cursor = true;
        self.cursor_reveal_mode = Some(match (self.cursor_reveal_mode, mode) {
            (Some(CursorRevealMode::Center), _) | (_, CursorRevealMode::Center) => {
                CursorRevealMode::Center
            }
            _ => CursorRevealMode::KeepVisible,
        });
    }

    pub fn cursor_reveal_mode(&self) -> Option<CursorRevealMode> {
        if !self.scroll_to_cursor {
            return None;
        }

        Some(
            self.cursor_reveal_mode
                .unwrap_or(CursorRevealMode::KeepVisible),
        )
    }

    pub fn clear_cursor_reveal(&mut self) {
        self.scroll_to_cursor = false;
        self.cursor_reveal_mode = None;
    }
}

impl SearchHighlightState {
    pub fn layout_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
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
    use super::{CursorRevealMode, EditorViewState, SearchHighlightState};
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

    #[test]
    fn identical_ime_output_is_not_republished() {
        let mut view = EditorViewState::new(7, false);
        let rect = egui::Rect::from_min_max(egui::pos2(1.0, 2.0), egui::pos2(11.0, 12.0));
        let cursor_rect = egui::Rect::from_min_max(egui::pos2(3.0, 4.0), egui::pos2(5.0, 16.0));

        assert!(view.mark_ime_output(rect, cursor_rect));
        assert!(!view.mark_ime_output(rect, cursor_rect));

        view.clear_ime_output();

        assert!(view.mark_ime_output(rect, cursor_rect));
    }

    #[test]
    fn highlight_layout_signature_changes_with_ranges() {
        let mut highlights = SearchHighlightState {
            ranges: std::iter::once(1..4).collect(),
            active_range_index: Some(0),
        };
        let initial = highlights.layout_signature();

        highlights.ranges.push(8..12);

        assert_ne!(highlights.layout_signature(), initial);
    }

    #[test]
    fn cursor_reveal_mode_defaults_legacy_scroll_flag_to_keep_visible() {
        let mut view = EditorViewState::new(7, false);
        view.scroll_to_cursor = true;

        assert_eq!(
            view.cursor_reveal_mode(),
            Some(CursorRevealMode::KeepVisible)
        );
    }

    #[test]
    fn center_cursor_reveal_dominates_keep_visible_until_cleared() {
        let mut view = EditorViewState::new(7, false);

        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
        view.request_cursor_reveal(CursorRevealMode::Center);
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);

        assert_eq!(view.cursor_reveal_mode(), Some(CursorRevealMode::Center));

        view.clear_cursor_reveal();

        assert_eq!(view.cursor_reveal_mode(), None);
        assert!(!view.scroll_to_cursor);
    }
}
