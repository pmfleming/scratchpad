use crate::app::domain::{BufferId, RenderedLayout};
use crate::app::ui::editor_content::native_editor::CursorRange;
use crate::app::ui::scrolling::{ScrollIntent, ScrollManager};
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

/// Cursor reveal preference. The actual scroll target rect is resolved by the
/// renderer once cursor geometry is known; the reveal is then dispatched as a
/// `ScrollIntent::Reveal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorRevealMode {
    /// Scroll only the minimum amount needed to keep the cursor visible.
    KeepVisible,
    /// Center the cursor (or place it near the centerband).
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
    /// Per-view scroll state. Single source of truth for scroll position,
    /// reveal requests, and viewport metrics.
    pub scroll: ScrollManager,
    /// Queued scroll intents to be applied on the next render frame.
    pub pending_intents: Vec<ScrollIntent>,
    /// Pending cursor-reveal mode. Resolved into a `ScrollIntent::Reveal` by
    /// the renderer once the cursor's display rect is known.
    pending_cursor_reveal: Option<CursorRevealMode>,
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
            scroll: ScrollManager::new(),
            pending_intents: Vec::new(),
            pending_cursor_reveal: None,
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
            scroll: ScrollManager::new(),
            pending_intents: Vec::new(),
            pending_cursor_reveal: None,
            published_ime_output: None,
            search_highlights: SearchHighlightState::default(),
        }
    }

    /// Queue a scroll intent. Applied during the next render frame in order.
    pub fn request_intent(&mut self, intent: ScrollIntent) {
        self.pending_intents.push(intent);
    }

    /// Request the cursor be revealed on the next render. `Center` dominates
    /// `KeepVisible` if both are requested before the next frame.
    pub fn request_cursor_reveal(&mut self, mode: CursorRevealMode) {
        self.pending_cursor_reveal = Some(match (self.pending_cursor_reveal, mode) {
            (Some(CursorRevealMode::Center), _) | (_, CursorRevealMode::Center) => {
                CursorRevealMode::Center
            }
            _ => CursorRevealMode::KeepVisible,
        });
    }

    pub fn cursor_reveal_mode(&self) -> Option<CursorRevealMode> {
        self.pending_cursor_reveal
    }

    pub fn clear_cursor_reveal(&mut self) {
        self.pending_cursor_reveal = None;
    }

    /// Pixel-space scroll offset derived from the per-view `ScrollManager`.
    /// Useful at the egui-wrapper boundary while phase 4 wiring is in flight.
    pub fn editor_pixel_offset(&self) -> egui::Vec2 {
        let metrics = self.scroll.metrics();
        let anchor = self.scroll.anchor();
        let y = (anchor.logical_line as f32 + anchor.display_row_offset)
            * metrics.row_height.max(0.0);
        egui::vec2(self.scroll.horizontal_px(), y)
    }

    /// Update the per-view scroll position from a pixel offset (e.g. coming
    /// out of the underlying egui ScrollArea). Resolves through the scroll
    /// manager's intent path for consistency.
    pub fn set_editor_pixel_offset(&mut self, offset: egui::Vec2) {
        use crate::app::ui::scrolling::{naive_anchor_to_row, naive_row_to_anchor, Axis};
        self.scroll.apply_intent(
            ScrollIntent::ScrollbarTo {
                axis: Axis::Y,
                offset_pixels: offset.y,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
        );
        self.scroll.apply_intent(
            ScrollIntent::ScrollbarTo {
                axis: Axis::X,
                offset_pixels: offset.x,
            },
            naive_anchor_to_row,
            naive_row_to_anchor,
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
}

impl SearchHighlightState {
    pub fn layout_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
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
    use super::{EditorViewState, SearchHighlightState};
    use eframe::egui;

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
}
