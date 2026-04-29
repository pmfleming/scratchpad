use crate::app::domain::buffer::AnchorId;
use crate::app::domain::{BufferId, RenderedLayout};
use crate::app::ui::editor_content::native_editor::CursorRange;
use crate::app::ui::scrolling::{DisplaySnapshot, ScrollAnchor, ScrollIntent, ScrollManager};
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
    /// Wrap-aware display-row snapshot derived from the most recently painted
    /// galley. Phase-3 viewport-first queries (`row_for_char_offset`,
    /// `viewport_slice`) read from this. None until the first frame paints.
    pub latest_display_snapshot: Option<DisplaySnapshot>,
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
    /// Most recently allocated piece-tree anchor backing the scroll anchor
    /// (when one exists). Released by `upgrade_scroll_anchor_to_piece` before
    /// allocating a replacement so the piece tree's anchor registry does not
    /// grow unbounded.
    last_piece_anchor: Option<AnchorId>,
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
            latest_display_snapshot: None,
            cursor_range: None,
            pending_cursor_range: None,
            scroll: ScrollManager::new(),
            pending_intents: Vec::new(),
            pending_cursor_reveal: None,
            last_piece_anchor: None,
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
            latest_display_snapshot: None,
            cursor_range: None,
            pending_cursor_range: None,
            scroll: ScrollManager::new(),
            pending_intents: Vec::new(),
            pending_cursor_reveal: None,
            last_piece_anchor: None,
            published_ime_output: None,
            search_highlights: SearchHighlightState::default(),
        }
    }

    /// Upgrade the scroll anchor to a piece-tree-backed `ScrollAnchor::Piece`,
    /// pinned at the current top-of-viewport char offset on the given buffer.
    /// Subsequent edits to the buffer above the viewport will keep the anchor
    /// pointing at the same content.
    ///
    /// Releases the previously-stored piece anchor if any (the back-channel
    /// `set_editor_pixel_offset` overwrites the manager's anchor with a
    /// logical anchor each frame, dropping its `AnchorId`; without an
    /// explicit release here the piece tree's anchor registry would grow
    /// unbounded). The `display_row_offset` is preserved across the upgrade.
    pub fn upgrade_scroll_anchor_to_piece(&mut self, buffer: &mut crate::app::domain::BufferState) {
        use crate::app::domain::AnchorBias;
        if matches!(self.scroll.anchor(), ScrollAnchor::Piece { .. }) {
            return;
        }
        let Some(snapshot) = self.latest_display_snapshot.as_ref() else {
            return;
        };
        let metrics = self.scroll.metrics();
        if metrics.row_height <= 0.0 {
            return;
        }
        // Resolve the current top display row to a char offset via the
        // snapshot, then create a stable anchor at that offset.
        let pixel_y = self.editor_pixel_offset().y;
        let top_row = (pixel_y / metrics.row_height).floor().max(0.0) as u32;
        let row_count = snapshot.row_count();
        if row_count == 0 {
            return;
        }
        let clamped_row = top_row.min(row_count.saturating_sub(1));
        let Some(range) =
            snapshot.row_char_range(crate::app::ui::scrolling::DisplayRow(clamped_row))
        else {
            return;
        };
        let char_offset = range.start as usize;
        // Release the previous piece anchor (if any) before allocating a
        // fresh one. See doc-comment above for why this is needed.
        if let Some(previous) = self.last_piece_anchor.take() {
            buffer
                .document_mut()
                .piece_tree_mut()
                .release_anchor(previous);
        }
        let anchor_id = buffer
            .document_mut()
            .piece_tree_mut()
            .create_anchor(char_offset, AnchorBias::Left);
        self.last_piece_anchor = Some(anchor_id);
        let frac = self.scroll.anchor().display_row_offset();
        self.scroll.replace_anchor(ScrollAnchor::Piece {
            anchor: anchor_id,
            display_row_offset: frac,
        });
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
    ///
    /// Note: for `ScrollAnchor::Piece` anchors this returns only the fractional
    /// row offset (≈ 0) because resolving the piece anchor requires the
    /// owning buffer. Use [`Self::editor_pixel_offset_resolved`] from a
    /// renderer that has buffer access for correct piece-anchor results.
    pub fn editor_pixel_offset(&self) -> egui::Vec2 {
        let metrics = self.scroll.metrics();
        let anchor = self.scroll.anchor();
        // For the v1 logical fallback we can compute pixel offset locally;
        // for piece-tree-backed anchors the renderer must resolve the anchor
        // through the active document + DisplaySnapshot, so we surface 0 here
        // and let the renderer override via `set_editor_pixel_offset`.
        let row = match anchor {
            crate::app::ui::scrolling::ScrollAnchor::Logical {
                logical_line,
                display_row_offset,
                ..
            } => logical_line as f32 + display_row_offset,
            crate::app::ui::scrolling::ScrollAnchor::Piece {
                display_row_offset, ..
            } => display_row_offset,
        };
        let y = row * metrics.row_height.max(0.0);
        egui::vec2(self.scroll.horizontal_px(), y)
    }

    /// Pixel-space scroll offset, resolving piece-tree-backed anchors through
    /// the given buffer + the view's latest `DisplaySnapshot`. Use this at
    /// renderer boundaries where the buffer is available so anchor stickiness
    /// is preserved across edits above the viewport.
    pub fn editor_pixel_offset_resolved(
        &self,
        buffer: &crate::app::domain::BufferState,
    ) -> egui::Vec2 {
        use crate::app::ui::scrolling::display_aware_anchor_to_row;
        let metrics = self.scroll.metrics();
        let snapshot = self.latest_display_snapshot.as_ref();
        let resolve = |id| buffer.document().piece_tree().anchor_position(id);
        let anchor_to_row = display_aware_anchor_to_row(snapshot, resolve);
        let row = anchor_to_row(self.scroll.anchor());
        let y = row * metrics.row_height.max(0.0);
        egui::vec2(self.scroll.horizontal_px(), y)
    }

    /// Update the per-view scroll position from a pixel offset (e.g. coming
    /// out of the underlying egui ScrollArea). Resolves through the scroll
    /// manager's intent path for consistency.
    pub fn set_editor_pixel_offset(&mut self, offset: egui::Vec2) {
        use crate::app::ui::scrolling::{Axis, naive_anchor_to_row, naive_row_to_anchor};
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
