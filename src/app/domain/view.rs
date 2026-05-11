mod anchors;
mod layout_cache;

use self::anchors::{
    AnchoredCursorRange, AnchoredSearchRange, release_anchors, resolve_cursor_anchor_range,
    sync_optional_cursor_anchor_range, take_cursor_anchors, take_search_anchors,
};
pub use layout_cache::{LayoutCache, LayoutCacheEntry, LayoutCacheKey};

use crate::app::domain::BufferId;
use crate::app::domain::buffer::{AnchorBias, AnchorId, AnchorOwner};
use crate::app::ui::editor_content::native_editor::CursorRange;
use crate::app::ui::scrolling::{DisplaySnapshot, ScrollAnchor, ScrollIntent, ScrollManager};
use eframe::egui;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

pub type ViewId = u64;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SearchHighlightState {
    pub ranges: Vec<Range<usize>>,
    pub active_range_index: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SearchReplacementPreview {
    pub entries: Vec<SearchReplacementPreviewEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SearchReplacementPreviewEntry {
    pub range: Range<usize>,
    pub replacement: String,
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
    /// Scroll only horizontally to keep the cursor visible.
    KeepHorizontalVisible,
    /// Center the cursor (or place it near the centerband).
    Center,
}

#[derive(Clone)]
pub struct EditorViewState {
    pub id: ViewId,
    pub buffer_id: BufferId,
    hot: EditorViewHotState,
    anchors: EditorViewAnchorState,
    published_ime_output: Option<PublishedImeOutput>,
}

#[derive(Clone)]
pub struct EditorViewHotState {
    pub show_line_numbers: bool,
    pub right_to_left_reading_order: bool,
    pub editor_has_focus: bool,
    /// Wrap-aware display-row snapshot derived from the most recently painted
    /// galley. Single source of truth for wrap-aware row data on the view.
    /// None until the first frame paints.
    pub latest_display_snapshot: Option<DisplaySnapshot>,
    /// Document revision tag for `latest_display_snapshot`; lets the
    /// `take_previous_snapshot`/restore dance only restore stale snapshots
    /// when the buffer hasn't changed under them.
    pub latest_display_snapshot_revision: Option<u64>,
    pub layout_cache: LayoutCache,
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
    pub ime_preedit: Option<String>,
    pub search_highlights: SearchHighlightState,
    pub search_replacement_preview: Option<SearchReplacementPreview>,
}

#[derive(Clone, Default)]
struct EditorViewAnchorState {
    /// Most recently allocated piece-tree anchor backing the scroll anchor
    /// (when one exists). Released by `upgrade_scroll_anchor_to_piece` before
    /// allocating a replacement so the piece tree's anchor registry does not
    /// grow unbounded.
    last_piece_anchor: Option<AnchorId>,
    cursor_anchor_range: Option<AnchoredCursorRange>,
    pending_cursor_anchor_range: Option<AnchoredCursorRange>,
    search_highlight_anchors: Vec<AnchoredSearchRange>,
}

impl Deref for EditorViewState {
    type Target = EditorViewHotState;

    fn deref(&self) -> &Self::Target {
        &self.hot
    }
}

impl DerefMut for EditorViewState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.hot
    }
}

impl EditorViewHotState {
    fn new(show_line_numbers: bool) -> Self {
        Self {
            show_line_numbers,
            right_to_left_reading_order: false,
            editor_has_focus: false,
            latest_display_snapshot: None,
            latest_display_snapshot_revision: None,
            layout_cache: LayoutCache::default(),
            cursor_range: None,
            pending_cursor_range: None,
            scroll: ScrollManager::new(),
            pending_intents: Vec::new(),
            pending_cursor_reveal: None,
            ime_preedit: None,
            search_highlights: SearchHighlightState::default(),
            search_replacement_preview: None,
        }
    }
}

impl EditorViewState {
    pub fn new(buffer_id: BufferId) -> Self {
        Self {
            id: next_view_id(),
            buffer_id,
            hot: EditorViewHotState::new(false),
            anchors: EditorViewAnchorState::default(),
            published_ime_output: None,
        }
    }

    pub fn restored(id: ViewId, buffer_id: BufferId, show_line_numbers: bool) -> Self {
        register_existing_view_id(id);
        Self {
            id,
            buffer_id,
            hot: EditorViewHotState::new(show_line_numbers),
            anchors: EditorViewAnchorState::default(),
            published_ime_output: None,
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
        let top_row = (pixel_y / metrics.row_height).floor().max(0.0);
        let row_count = snapshot.row_count();
        if row_count == 0 {
            return;
        }
        let Some(snapshot_row) = snapshot.row_for_document_row(top_row) else {
            return;
        };
        let Some(range) = snapshot.row_char_range(snapshot_row) else {
            return;
        };
        let char_offset = range.start as usize;
        // Release the previous piece anchor (if any) before allocating a
        // fresh one. See doc-comment above for why this is needed.
        if let Some(previous) = self.anchors.last_piece_anchor.take() {
            buffer
                .document_mut()
                .piece_tree_mut()
                .release_anchor(previous);
        }
        let anchor_id = buffer
            .document_mut()
            .piece_tree_mut()
            .create_anchor_with_owner(
                char_offset,
                AnchorBias::Left,
                AnchorOwner::view_scroll(self.id),
            );
        self.anchors.last_piece_anchor = Some(anchor_id);
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
            (Some(CursorRevealMode::KeepVisible), _) | (_, CursorRevealMode::KeepVisible) => {
                CursorRevealMode::KeepVisible
            }
            _ => CursorRevealMode::KeepHorizontalVisible,
        });
    }

    pub fn cursor_reveal_mode(&self) -> Option<CursorRevealMode> {
        self.pending_cursor_reveal
    }

    pub fn clear_cursor_reveal(&mut self) {
        self.pending_cursor_reveal = None;
    }

    /// Take the view-owned piece anchor so its buffer can release it before
    /// this view is cleared, closed, or detached from the buffer context.
    pub fn take_piece_anchor_for_release(&mut self) -> Option<AnchorId> {
        let anchor = self.anchors.last_piece_anchor.take()?;
        if self.scroll.anchor().piece_anchor() == Some(anchor) {
            self.scroll.replace_anchor(ScrollAnchor::TOP);
        }
        Some(anchor)
    }

    pub fn take_runtime_anchors_for_release(&mut self) -> Vec<AnchorId> {
        let mut anchors = Vec::new();
        if let Some(anchor) = self.take_piece_anchor_for_release() {
            anchors.push(anchor);
        }
        anchors.extend(take_cursor_anchors(&mut self.anchors.cursor_anchor_range));
        anchors.extend(take_cursor_anchors(
            &mut self.anchors.pending_cursor_anchor_range,
        ));
        anchors.extend(take_search_anchors(
            &mut self.anchors.search_highlight_anchors,
        ));
        self.search_highlights.ranges.clear();
        self.search_highlights.active_range_index = None;
        self.search_replacement_preview = None;
        anchors
    }

    pub fn resolve_anchored_ranges(&mut self, buffer: &crate::app::domain::BufferState) {
        if let Some(cursor_range) =
            resolve_cursor_anchor_range(self.anchors.cursor_anchor_range, buffer)
        {
            self.cursor_range = Some(cursor_range);
        }
        if let Some(cursor_range) =
            resolve_cursor_anchor_range(self.anchors.pending_cursor_anchor_range, buffer)
        {
            self.pending_cursor_range = Some(cursor_range);
        }
        self.resolve_search_highlight_anchors(buffer);
    }

    pub fn sync_cursor_anchors_from_ranges(
        &mut self,
        buffer: &mut crate::app::domain::BufferState,
    ) {
        sync_optional_cursor_anchor_range(
            self.id,
            buffer,
            self.cursor_range,
            &mut self.anchors.cursor_anchor_range,
        );
        sync_optional_cursor_anchor_range(
            self.id,
            buffer,
            self.pending_cursor_range,
            &mut self.anchors.pending_cursor_anchor_range,
        );
    }

    pub fn set_cursor_range_anchored(
        &mut self,
        buffer: &mut crate::app::domain::BufferState,
        cursor_range: CursorRange,
    ) {
        self.cursor_range = Some(cursor_range);
        sync_optional_cursor_anchor_range(
            self.id,
            buffer,
            self.cursor_range,
            &mut self.anchors.cursor_anchor_range,
        );
    }

    pub fn set_pending_cursor_range_anchored(
        &mut self,
        buffer: &mut crate::app::domain::BufferState,
        cursor_range: CursorRange,
    ) {
        self.pending_cursor_range = Some(cursor_range);
        sync_optional_cursor_anchor_range(
            self.id,
            buffer,
            self.pending_cursor_range,
            &mut self.anchors.pending_cursor_anchor_range,
        );
    }

    pub fn set_search_highlights_anchored(
        &mut self,
        buffer: &mut crate::app::domain::BufferState,
        highlights: SearchHighlightState,
    ) {
        if self.search_highlights == highlights {
            return;
        }
        release_anchors(
            buffer,
            take_search_anchors(&mut self.anchors.search_highlight_anchors),
        );
        let mut highlight_anchors = Vec::with_capacity(highlights.ranges.len());
        for range in &highlights.ranges {
            if range.start >= range.end {
                continue;
            }
            let start = buffer
                .document_mut()
                .piece_tree_mut()
                .create_anchor_with_owner(
                    range.start,
                    AnchorBias::Left,
                    AnchorOwner::search_endpoint(self.id),
                );
            let end = buffer
                .document_mut()
                .piece_tree_mut()
                .create_anchor_with_owner(
                    range.end,
                    AnchorBias::Right,
                    AnchorOwner::search_endpoint(self.id),
                );
            highlight_anchors.push(AnchoredSearchRange { start, end });
        }
        self.search_highlights = highlights;
        self.anchors.search_highlight_anchors = highlight_anchors;
    }

    pub fn clear_search_highlights_for_release(&mut self) -> Vec<AnchorId> {
        self.search_highlights.ranges.clear();
        self.search_highlights.active_range_index = None;
        self.search_replacement_preview = None;
        take_search_anchors(&mut self.anchors.search_highlight_anchors)
    }

    pub fn set_search_replacement_preview(&mut self, preview: Option<SearchReplacementPreview>) {
        if self.search_replacement_preview == preview {
            return;
        }
        self.search_replacement_preview = preview;
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
        let snapshot = self.latest_display_snapshot.as_ref();
        let resolve = |id| buffer.document().piece_tree().anchor_position(id);
        let anchor_to_row = display_aware_anchor_to_row(snapshot, resolve);
        let y = self.scroll.pixel_offset_y(anchor_to_row);
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

    /// Update the per-view scroll position from a pixel offset while using
    /// the latest display snapshot to seed a piece-tree-backed vertical
    /// anchor. Falls back to logical mapping until a snapshot is available.
    pub fn set_editor_pixel_offset_resolved(
        &mut self,
        buffer: &mut crate::app::domain::BufferState,
        offset: egui::Vec2,
    ) {
        let Some(anchor) = self.anchor_for_pixel_offset(buffer, offset) else {
            self.set_editor_pixel_offset(offset);
            return;
        };

        self.scroll.replace_anchor(anchor);
        self.set_horizontal_pixel_offset(offset.x);
    }

    fn anchor_for_pixel_offset(
        &mut self,
        buffer: &mut crate::app::domain::BufferState,
        offset: egui::Vec2,
    ) -> Option<ScrollAnchor> {
        use crate::app::domain::AnchorBias;
        let (row_start, display_row_offset) = {
            let snapshot = self.hot.latest_display_snapshot.as_ref()?;
            let metrics = self.hot.scroll.metrics();
            if metrics.row_height <= 0.0 || snapshot.row_count() == 0 {
                return None;
            }

            let row = (offset.y / metrics.row_height).max(0.0);
            // The display snapshot only covers the rendered slice (visible rows +
            // overscan). If the requested row is outside that window, fall back
            // to the naive logical mapping in `set_editor_pixel_offset` — the
            // piece anchor would otherwise be capped to the slice's last row,
            // which silently bounds vertical scroll to the slice end.
            let snapshot_row = snapshot.row_for_document_row(row)?;
            let row_range = snapshot.row_char_range(snapshot_row)?;
            let document_row = snapshot
                .document_row_for_snapshot_row(snapshot_row)
                .unwrap_or_else(|| row.floor());
            (row_range.start as usize, (row - document_row).max(0.0))
        };
        if let Some(previous) = self.anchors.last_piece_anchor.take() {
            buffer
                .document_mut()
                .piece_tree_mut()
                .release_anchor(previous);
        }
        let anchor_id = buffer
            .document_mut()
            .piece_tree_mut()
            .create_anchor_with_owner(
                row_start,
                AnchorBias::Left,
                AnchorOwner::view_scroll(self.id),
            );
        self.anchors.last_piece_anchor = Some(anchor_id);
        Some(ScrollAnchor::Piece {
            anchor: anchor_id,
            display_row_offset,
        })
    }

    fn set_horizontal_pixel_offset(&mut self, offset_x: f32) {
        use crate::app::ui::scrolling::{Axis, naive_anchor_to_row, naive_row_to_anchor};
        self.scroll.apply_intent(
            ScrollIntent::ScrollbarTo {
                axis: Axis::X,
                offset_pixels: offset_x,
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

impl EditorViewState {
    fn resolve_search_highlight_anchors(&mut self, buffer: &crate::app::domain::BufferState) {
        if self.anchors.search_highlight_anchors.is_empty() {
            return;
        }
        let piece_tree = buffer.document().piece_tree();
        let mut ranges = Vec::with_capacity(self.anchors.search_highlight_anchors.len());
        let mut active_range_index = None;
        for (index, anchored) in self.anchors.search_highlight_anchors.iter().enumerate() {
            let Some(start) = piece_tree.anchor_position(anchored.start) else {
                continue;
            };
            let Some(end) = piece_tree.anchor_position(anchored.end) else {
                continue;
            };
            if start >= end {
                continue;
            }
            if self.search_highlights.active_range_index == Some(index) {
                active_range_index = Some(ranges.len());
            }
            ranges.push(start..end);
        }
        self.search_highlights.ranges = ranges;
        self.search_highlights.active_range_index = active_range_index;
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
