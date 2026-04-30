use crate::app::domain::BufferId;
use crate::app::domain::buffer::{AnchorBias, AnchorId, AnchorOwner};
use crate::app::ui::editor_content::native_editor::CursorRange;
use crate::app::ui::scrolling::{DisplaySnapshot, ScrollAnchor, ScrollIntent, ScrollManager};
use eframe::egui;
use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

pub type ViewId = u64;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SearchHighlightState {
    pub ranges: Vec<Range<usize>>,
    pub active_range_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayoutCacheKey {
    pub revision: u64,
    pub char_range: Range<usize>,
    pub font_family: String,
    pub font_size_bits: u32,
    pub wrap_width_bits: u32,
    pub word_wrap: bool,
    pub text_color: egui::Color32,
    pub dark_mode: bool,
    pub selection_range: Option<Range<usize>>,
    pub search_highlights: SearchHighlightState,
}

#[derive(Clone)]
pub struct LayoutCacheEntry {
    pub key: LayoutCacheKey,
    pub galley: Arc<egui::Galley>,
    pub input_bytes: usize,
}

#[derive(Clone, Default)]
pub struct LayoutCache {
    entries: VecDeque<LayoutCacheEntry>,
    bytes: usize,
}

impl LayoutCache {
    const MAX_ENTRIES: usize = 8;
    const MAX_BYTES: usize = 4 * 1024 * 1024;

    pub fn get(&mut self, key: &LayoutCacheKey) -> Option<Arc<egui::Galley>> {
        let index = self.entries.iter().position(|entry| &entry.key == key)?;
        let entry = self.entries.remove(index)?;
        let galley = entry.galley.clone();
        self.entries.push_front(entry);
        Some(galley)
    }

    pub fn insert(&mut self, key: LayoutCacheKey, galley: Arc<egui::Galley>, input_bytes: usize) {
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key)
            && let Some(existing) = self.entries.remove(index)
        {
            self.bytes = self.bytes.saturating_sub(existing.input_bytes);
        }
        self.bytes = self.bytes.saturating_add(input_bytes);
        self.entries.push_front(LayoutCacheEntry {
            key,
            galley,
            input_bytes,
        });
        self.evict_over_budget();
    }

    pub fn retain_revision(&mut self, revision: u64) {
        self.entries.retain(|entry| entry.key.revision == revision);
        self.bytes = self.entries.iter().map(|entry| entry.input_bytes).sum();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn evict_over_budget(&mut self) {
        while self.entries.len() > Self::MAX_ENTRIES || self.bytes > Self::MAX_BYTES {
            let Some(entry) = self.entries.pop_back() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(entry.input_bytes);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AnchoredEndpoint {
    anchor: AnchorId,
    prefer_next_row: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AnchoredCursorRange {
    primary: AnchoredEndpoint,
    secondary: AnchoredEndpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AnchoredSearchRange {
    start: AnchorId,
    end: AnchorId,
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
    pub show_line_numbers: bool,
    pub show_control_chars: bool,
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
    /// Most recently allocated piece-tree anchor backing the scroll anchor
    /// (when one exists). Released by `upgrade_scroll_anchor_to_piece` before
    /// allocating a replacement so the piece tree's anchor registry does not
    /// grow unbounded.
    last_piece_anchor: Option<AnchorId>,
    cursor_anchor_range: Option<AnchoredCursorRange>,
    pending_cursor_anchor_range: Option<AnchoredCursorRange>,
    search_highlight_anchors: Vec<AnchoredSearchRange>,
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
            latest_display_snapshot: None,
            latest_display_snapshot_revision: None,
            layout_cache: LayoutCache::default(),
            cursor_range: None,
            pending_cursor_range: None,
            scroll: ScrollManager::new(),
            pending_intents: Vec::new(),
            pending_cursor_reveal: None,
            last_piece_anchor: None,
            cursor_anchor_range: None,
            pending_cursor_anchor_range: None,
            search_highlight_anchors: Vec::new(),
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
            latest_display_snapshot: None,
            latest_display_snapshot_revision: None,
            layout_cache: LayoutCache::default(),
            cursor_range: None,
            pending_cursor_range: None,
            scroll: ScrollManager::new(),
            pending_intents: Vec::new(),
            pending_cursor_reveal: None,
            last_piece_anchor: None,
            cursor_anchor_range: None,
            pending_cursor_anchor_range: None,
            search_highlight_anchors: Vec::new(),
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
            .create_anchor_with_owner(
                char_offset,
                AnchorBias::Left,
                AnchorOwner::view_scroll(self.id),
            );
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
        let anchor = self.last_piece_anchor.take()?;
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
        anchors.extend(take_cursor_anchors(&mut self.cursor_anchor_range));
        anchors.extend(take_cursor_anchors(&mut self.pending_cursor_anchor_range));
        anchors.extend(take_search_anchors(&mut self.search_highlight_anchors));
        self.search_highlights.ranges.clear();
        self.search_highlights.active_range_index = None;
        anchors
    }

    pub fn resolve_anchored_ranges(&mut self, buffer: &crate::app::domain::BufferState) {
        if let Some(cursor_range) = resolve_cursor_anchor_range(self.cursor_anchor_range, buffer) {
            self.cursor_range = Some(cursor_range);
        }
        if let Some(cursor_range) =
            resolve_cursor_anchor_range(self.pending_cursor_anchor_range, buffer)
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
            &mut self.cursor_anchor_range,
        );
        sync_optional_cursor_anchor_range(
            self.id,
            buffer,
            self.pending_cursor_range,
            &mut self.pending_cursor_anchor_range,
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
            &mut self.cursor_anchor_range,
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
            &mut self.pending_cursor_anchor_range,
        );
    }

    pub fn set_search_highlights_anchored(
        &mut self,
        buffer: &mut crate::app::domain::BufferState,
        highlights: SearchHighlightState,
    ) {
        release_anchors(
            buffer,
            take_search_anchors(&mut self.search_highlight_anchors),
        );
        self.search_highlights = highlights;
        for range in &self.search_highlights.ranges {
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
            self.search_highlight_anchors
                .push(AnchoredSearchRange { start, end });
        }
    }

    pub fn clear_search_highlights_for_release(&mut self) -> Vec<AnchorId> {
        self.search_highlights.ranges.clear();
        self.search_highlights.active_range_index = None;
        take_search_anchors(&mut self.search_highlight_anchors)
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
        let snapshot = self.latest_display_snapshot.as_ref()?;
        let metrics = self.scroll.metrics();
        if metrics.row_height <= 0.0 || snapshot.row_count() == 0 {
            return None;
        }

        let row = (offset.y / metrics.row_height).max(0.0);
        let row_index = row.floor() as u32;
        let clamped_row = row_index.min(snapshot.row_count().saturating_sub(1));
        let row_range =
            snapshot.row_char_range(crate::app::ui::scrolling::DisplayRow(clamped_row))?;
        if let Some(previous) = self.last_piece_anchor.take() {
            buffer
                .document_mut()
                .piece_tree_mut()
                .release_anchor(previous);
        }
        let anchor_id = buffer
            .document_mut()
            .piece_tree_mut()
            .create_anchor_with_owner(
                row_range.start as usize,
                AnchorBias::Left,
                AnchorOwner::view_scroll(self.id),
            );
        self.last_piece_anchor = Some(anchor_id);
        Some(ScrollAnchor::Piece {
            anchor: anchor_id,
            display_row_offset: (row - clamped_row as f32).max(0.0),
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

#[cfg(test)]
mod layout_cache_tests {
    use super::{LayoutCache, LayoutCacheKey, SearchHighlightState};
    use eframe::egui;
    use std::sync::Arc;

    fn key(revision: u64, start: usize) -> LayoutCacheKey {
        LayoutCacheKey {
            revision,
            char_range: start..start + 10,
            font_family: "Monospace".to_owned(),
            font_size_bits: 14.0_f32.to_bits(),
            wrap_width_bits: f32::INFINITY.to_bits(),
            word_wrap: false,
            text_color: egui::Color32::WHITE,
            dark_mode: true,
            selection_range: None,
            search_highlights: SearchHighlightState::default(),
        }
    }

    fn galley() -> Arc<egui::Galley> {
        let ctx = egui::Context::default();
        let mut galley = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            galley = Some(ui.fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    "cached".to_owned(),
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    f32::INFINITY,
                ))
            }));
        });
        galley.expect("galley")
    }

    #[test]
    fn layout_cache_returns_matching_revision_and_range() {
        let mut cache = LayoutCache::default();
        let cached_key = key(7, 10);
        cache.insert(cached_key.clone(), galley(), 6);

        assert!(cache.get(&cached_key).is_some());
        assert!(cache.get(&key(8, 10)).is_none());
    }

    #[test]
    fn layout_cache_evicts_stale_revisions() {
        let mut cache = LayoutCache::default();
        cache.insert(key(7, 0), galley(), 6);
        cache.insert(key(8, 0), galley(), 6);

        cache.retain_revision(8);

        assert_eq!(cache.len(), 1);
        assert!(cache.get(&key(7, 0)).is_none());
        assert!(cache.get(&key(8, 0)).is_some());
    }
}

impl SearchHighlightState {
    pub fn layout_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

fn sync_optional_cursor_anchor_range(
    view_id: ViewId,
    buffer: &mut crate::app::domain::BufferState,
    cursor_range: Option<CursorRange>,
    anchored: &mut Option<AnchoredCursorRange>,
) {
    if resolve_cursor_anchor_range(*anchored, buffer) == cursor_range {
        return;
    }
    release_anchors(buffer, take_cursor_anchors(anchored));
    let Some(cursor_range) = cursor_range else {
        return;
    };
    *anchored = Some(create_cursor_anchor_range(view_id, buffer, cursor_range));
}

fn create_cursor_anchor_range(
    view_id: ViewId,
    buffer: &mut crate::app::domain::BufferState,
    cursor_range: CursorRange,
) -> AnchoredCursorRange {
    let (start, end) = cursor_range.sorted_indices();
    AnchoredCursorRange {
        primary: create_cursor_endpoint_anchor(
            buffer,
            cursor_range.primary.index,
            cursor_endpoint_bias(cursor_range.primary.index, start, end),
            AnchorOwner::cursor(view_id),
            cursor_range.primary.prefer_next_row,
        ),
        secondary: create_cursor_endpoint_anchor(
            buffer,
            cursor_range.secondary.index,
            cursor_endpoint_bias(cursor_range.secondary.index, start, end),
            AnchorOwner::selection_endpoint(view_id),
            cursor_range.secondary.prefer_next_row,
        ),
    }
}

fn cursor_endpoint_bias(index: usize, start: usize, end: usize) -> AnchorBias {
    if start == end || index >= end {
        AnchorBias::Right
    } else {
        AnchorBias::Left
    }
}

fn create_cursor_endpoint_anchor(
    buffer: &mut crate::app::domain::BufferState,
    index: usize,
    bias: AnchorBias,
    owner: AnchorOwner,
    prefer_next_row: bool,
) -> AnchoredEndpoint {
    let anchor = buffer
        .document_mut()
        .piece_tree_mut()
        .create_anchor_with_owner(index, bias, owner);
    AnchoredEndpoint {
        anchor,
        prefer_next_row,
    }
}

fn resolve_cursor_anchor_range(
    anchored: Option<AnchoredCursorRange>,
    buffer: &crate::app::domain::BufferState,
) -> Option<CursorRange> {
    let anchored = anchored?;
    let piece_tree = buffer.document().piece_tree();
    Some(CursorRange {
        primary: crate::app::ui::editor_content::native_editor::CharCursor {
            index: piece_tree.anchor_position(anchored.primary.anchor)?,
            prefer_next_row: anchored.primary.prefer_next_row,
        },
        secondary: crate::app::ui::editor_content::native_editor::CharCursor {
            index: piece_tree.anchor_position(anchored.secondary.anchor)?,
            prefer_next_row: anchored.secondary.prefer_next_row,
        },
    })
}

fn take_cursor_anchors(anchored: &mut Option<AnchoredCursorRange>) -> Vec<AnchorId> {
    anchored
        .take()
        .map(|range| vec![range.primary.anchor, range.secondary.anchor])
        .unwrap_or_default()
}

fn take_search_anchors(anchors: &mut Vec<AnchoredSearchRange>) -> Vec<AnchorId> {
    anchors
        .drain(..)
        .flat_map(|range| [range.start, range.end])
        .collect()
}

fn release_anchors(buffer: &mut crate::app::domain::BufferState, anchors: Vec<AnchorId>) {
    for anchor in anchors {
        buffer
            .document_mut()
            .piece_tree_mut()
            .release_anchor(anchor);
    }
}

impl EditorViewState {
    fn resolve_search_highlight_anchors(&mut self, buffer: &crate::app::domain::BufferState) {
        if self.search_highlight_anchors.is_empty() {
            return;
        }
        let piece_tree = buffer.document().piece_tree();
        let mut ranges = Vec::with_capacity(self.search_highlight_anchors.len());
        let mut active_range_index = None;
        for (index, anchored) in self.search_highlight_anchors.iter().enumerate() {
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

#[cfg(test)]
mod tests {
    use super::{EditorViewState, SearchHighlightState};
    use crate::app::domain::{AnchorOwner, BufferState};
    use crate::app::ui::scrolling::{
        ContentExtent, DisplaySnapshot, ScrollAnchor, ViewportMetrics,
    };
    use eframe::egui;

    fn snapshot_for(text: &str) -> DisplaySnapshot {
        let ctx = egui::Context::default();
        let mut galley = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            galley = Some(ui.fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    text.to_owned(),
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    f32::INFINITY,
                ))
            }));
        });
        DisplaySnapshot::from_galley(galley.expect("galley"), 10.0)
    }

    fn install_snapshot(view: &mut EditorViewState, snapshot: DisplaySnapshot) {
        view.scroll.set_metrics(ViewportMetrics {
            viewport_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 40.0)),
            row_height: snapshot.row_height(),
            column_width: 5.0,
            visible_rows: 4,
            visible_columns: 40,
        });
        view.scroll.set_extent(ContentExtent {
            display_rows: snapshot.row_count(),
            height: snapshot.content_height(),
            max_line_width: snapshot.max_line_width(),
        });
        view.latest_display_snapshot = Some(snapshot);
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
    fn resolved_pixel_offset_seeds_view_owned_piece_anchor() {
        let text = "zero\none\ntwo\nthree\nfour\nfive\n";
        let mut buffer = BufferState::new("notes.txt".to_owned(), text.to_owned(), None);
        let mut view = EditorViewState::new(buffer.id, false);
        install_snapshot(&mut view, snapshot_for(text));

        view.set_editor_pixel_offset_resolved(&mut buffer, egui::vec2(12.0, 20.0));

        let ScrollAnchor::Piece { anchor, .. } = view.scroll.anchor() else {
            panic!("expected piece-backed scroll anchor");
        };
        assert_eq!(buffer.document().piece_tree().live_anchor_count(), 1);
        assert_eq!(
            buffer.document().piece_tree().anchor_owner(anchor),
            Some(AnchorOwner::view_scroll(view.id))
        );
        assert_eq!(view.editor_pixel_offset_resolved(&buffer).y, 20.0);

        view.set_editor_pixel_offset_resolved(&mut buffer, egui::vec2(4.0, 30.0));

        assert_eq!(buffer.document().piece_tree().live_anchor_count(), 1);
        assert_eq!(view.editor_pixel_offset_resolved(&buffer).y, 30.0);
    }

    #[test]
    fn cursor_and_selection_endpoint_anchors_track_edits_above_range() {
        let mut buffer =
            BufferState::new("notes.txt".to_owned(), "alpha beta gamma".to_owned(), None);
        let mut view = EditorViewState::new(buffer.id, false);
        let selected = crate::app::ui::editor_content::native_editor::CursorRange::two(6, 10);

        view.set_cursor_range_anchored(&mut buffer, selected);

        assert_eq!(buffer.document().piece_tree().live_anchor_count(), 2);
        let anchored = view.cursor_anchor_range.expect("cursor anchors");
        assert_eq!(
            buffer
                .document()
                .piece_tree()
                .anchor_owner(anchored.primary.anchor),
            Some(AnchorOwner::cursor(view.id))
        );
        assert_eq!(
            buffer
                .document()
                .piece_tree()
                .anchor_owner(anchored.secondary.anchor),
            Some(AnchorOwner::selection_endpoint(view.id))
        );

        buffer.document_mut().insert_direct(0, "zz ");
        view.resolve_anchored_ranges(&buffer);

        assert_eq!(
            view.cursor_range
                .expect("resolved cursor")
                .as_sorted_char_range(),
            9..13
        );
    }

    #[test]
    fn search_endpoint_anchors_track_edits_and_release_cleanly() {
        let mut buffer =
            BufferState::new("notes.txt".to_owned(), "alpha beta gamma".to_owned(), None);
        let mut view = EditorViewState::new(buffer.id, false);

        view.set_search_highlights_anchored(
            &mut buffer,
            SearchHighlightState {
                ranges: std::iter::once(6..10).collect(),
                active_range_index: Some(0),
            },
        );

        assert_eq!(buffer.document().piece_tree().live_anchor_count(), 2);
        let anchored = view.search_highlight_anchors[0];
        assert_eq!(
            buffer.document().piece_tree().anchor_owner(anchored.start),
            Some(AnchorOwner::search_endpoint(view.id))
        );
        assert_eq!(
            buffer.document().piece_tree().anchor_owner(anchored.end),
            Some(AnchorOwner::search_endpoint(view.id))
        );

        buffer.document_mut().insert_direct(0, "zz ");
        view.resolve_anchored_ranges(&buffer);

        assert_eq!(view.search_highlights.ranges, vec![9..13]);
        assert_eq!(view.search_highlights.active_range_index, Some(0));

        for anchor in view.clear_search_highlights_for_release() {
            buffer
                .document_mut()
                .piece_tree_mut()
                .release_anchor(anchor);
        }
        assert_eq!(buffer.document().piece_tree().live_anchor_count(), 0);
    }
}
