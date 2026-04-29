use crate::app::domain::{BufferId, RenderedLayout};
use crate::app::ui::editor_content::native_editor::CursorRange;
use crate::app::ui::scrolling::{
    DisplayMapCache, DisplaySnapshot, ScrollAnchor, ScrollIntent, ScrollManager,
};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorRenderNotice {
    message: String,
}

/// View-owned metadata for the display rows currently published by the
/// renderer. This is the shared source consumed by gutter, status bar, split
/// preview, and artifact mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedViewport {
    pub row_range: Range<usize>,
    pub line_range: Range<usize>,
    pub layout_row_offset: usize,
}

impl EditorRenderNotice {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

/// High-level reveal request. The actual scroll target rect is resolved by the
/// renderer once cursor geometry is known; the reveal is then dispatched as a
/// `ScrollIntent::Reveal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevealRequest {
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
    pub latest_display_snapshot: Option<DisplaySnapshot>,
    pub display_map_cache: DisplayMapCache,
    pub latest_layout_revision: Option<u64>,
    pub cursor_range: Option<CursorRange>,
    pub pending_cursor_range: Option<CursorRange>,
    /// Per-view scroll state. Single source of truth for scroll position,
    /// reveal requests, and viewport metrics.
    pub scroll: ScrollManager,
    /// Queued scroll intents to be applied on the next render frame.
    pub pending_intents: Vec<ScrollIntent>,
    /// Pending reveal request. Resolved into a `ScrollIntent::Reveal` by
    /// the renderer once the cursor's display rect is known.
    pending_reveal_request: Option<RevealRequest>,
    published_ime_output: Option<PublishedImeOutput>,
    published_viewport: Option<PublishedViewport>,
    render_notice: Option<EditorRenderNotice>,
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
            latest_display_snapshot: None,
            display_map_cache: DisplayMapCache::default(),
            latest_layout_revision: None,
            cursor_range: None,
            pending_cursor_range: None,
            scroll: ScrollManager::new(),
            pending_intents: Vec::new(),
            pending_reveal_request: None,
            published_ime_output: None,
            published_viewport: None,
            render_notice: None,
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
            latest_display_snapshot: None,
            display_map_cache: DisplayMapCache::default(),
            latest_layout_revision: None,
            cursor_range: None,
            pending_cursor_range: None,
            scroll: ScrollManager::new(),
            pending_intents: Vec::new(),
            pending_reveal_request: None,
            published_ime_output: None,
            published_viewport: None,
            render_notice: None,
            search_highlights: SearchHighlightState::default(),
        }
    }

    /// Queue a scroll intent. Applied during the next render frame in order.
    pub fn request_intent(&mut self, intent: ScrollIntent) {
        self.pending_intents.push(intent);
    }

    /// Request a reveal on the next render. `Center` dominates `KeepVisible`
    /// if both are requested before the next frame.
    pub fn request_reveal(&mut self, request: RevealRequest) {
        self.pending_reveal_request = Some(match (self.pending_reveal_request, request) {
            (Some(RevealRequest::Center), _) | (_, RevealRequest::Center) => RevealRequest::Center,
            _ => RevealRequest::KeepVisible,
        });
    }

    pub fn reveal_request(&self) -> Option<RevealRequest> {
        self.pending_reveal_request
    }

    pub fn clear_reveal_request(&mut self) {
        self.pending_reveal_request = None;
    }

    /// Pixel-space scroll offset derived from the per-view `ScrollManager`.
    /// Uses the latest `RenderedLayout` when available to translate the anchor
    /// through the actual display-row map (handles soft wrap correctly);
    /// falls back to the naive identity map until layout is published.
    pub fn editor_pixel_offset(&self) -> egui::Vec2 {
        let metrics = self.scroll.metrics();
        let row_height = metrics.row_height.max(0.0);
        let row = self.display_anchor_to_row(self.scroll.anchor());
        egui::vec2(self.scroll.horizontal_px(), row * row_height)
    }

    /// Update the per-view scroll position from a pixel offset (e.g. coming
    /// out of the underlying egui ScrollArea). Resolves through the scroll
    /// manager's intent path for consistency.
    pub fn set_editor_pixel_offset(&mut self, offset: egui::Vec2) {
        use crate::app::ui::scrolling::Axis;
        self.apply_scroll_intent(ScrollIntent::ScrollbarTo {
            axis: Axis::Y,
            offset_pixels: offset.y,
        });
        self.apply_scroll_intent(ScrollIntent::ScrollbarTo {
            axis: Axis::X,
            offset_pixels: offset.x,
        });
    }

    pub fn apply_pending_scroll_intents(&mut self) {
        for intent in std::mem::take(&mut self.pending_intents) {
            self.apply_scroll_intent(intent);
        }
    }

    pub fn tick_edge_autoscroll(&mut self, dt: f32) {
        let layout = self.latest_layout.clone();
        let snapshot = self.latest_display_snapshot.clone();
        let to_row =
            move |anchor| display_anchor_to_row(snapshot.as_ref(), layout.as_ref(), anchor);
        let layout2 = self.latest_layout.clone();
        let snapshot2 = self.latest_display_snapshot.clone();
        let to_anchor = move |row| display_row_to_anchor(snapshot2.as_ref(), layout2.as_ref(), row);
        self.scroll.tick_edge_autoscroll(dt, &to_row, &to_anchor);
    }

    fn apply_scroll_intent(&mut self, intent: ScrollIntent) {
        let layout = self.latest_layout.clone();
        let snapshot = self.latest_display_snapshot.clone();
        let to_row =
            move |anchor| display_anchor_to_row(snapshot.as_ref(), layout.as_ref(), anchor);
        let layout2 = self.latest_layout.clone();
        let snapshot2 = self.latest_display_snapshot.clone();
        let to_anchor = move |row| display_row_to_anchor(snapshot2.as_ref(), layout2.as_ref(), row);
        self.scroll.apply_intent(intent, &to_row, &to_anchor);
    }

    fn display_anchor_to_row(&self, anchor: crate::app::ui::scrolling::ScrollAnchor) -> f32 {
        display_anchor_to_row(
            self.latest_display_snapshot.as_ref(),
            self.latest_layout.as_ref(),
            anchor,
        )
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

    pub fn publish_viewport(&mut self, viewport: PublishedViewport) {
        self.published_viewport = Some(viewport);
    }

    pub fn clear_published_viewport(&mut self) {
        self.published_viewport = None;
    }

    pub fn published_viewport(&self) -> Option<&PublishedViewport> {
        self.published_viewport.as_ref()
    }

    pub fn set_render_notice(&mut self, notice: EditorRenderNotice) {
        if self.render_notice.as_ref() != Some(&notice) {
            self.render_notice = Some(notice);
        }
    }

    pub fn clear_render_notice(&mut self) {
        self.render_notice = None;
    }

    pub fn render_notice(&self) -> Option<&EditorRenderNotice> {
        self.render_notice.as_ref()
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

fn display_anchor_to_row(
    snapshot: Option<&DisplaySnapshot>,
    layout: Option<&RenderedLayout>,
    anchor: ScrollAnchor,
) -> f32 {
    if let Some(snapshot) = snapshot
        && let Some(row) = snapshot.display_row_for_logical_line(anchor.logical_line as usize)
    {
        return row as f32 + anchor.display_row_offset;
    }
    match layout {
        Some(layout) => layout
            .display_row_for_logical_line(anchor.logical_line as usize)
            .map(|row| row as f32 + anchor.display_row_offset)
            .unwrap_or_else(|| anchor.logical_line as f32 + anchor.display_row_offset),
        None => anchor.logical_line as f32 + anchor.display_row_offset,
    }
}

fn display_row_to_anchor(
    snapshot: Option<&DisplaySnapshot>,
    layout: Option<&RenderedLayout>,
    row: f32,
) -> ScrollAnchor {
    if let Some(snapshot) = snapshot {
        let (logical_line, frac) = snapshot.anchor_at_display_row(row);
        return ScrollAnchor {
            logical_line: logical_line as u32,
            byte_in_line: 0,
            display_row_offset: frac,
        };
    }
    match layout {
        Some(layout) => {
            let (logical_line, frac) = layout.anchor_at_display_row(row);
            ScrollAnchor {
                logical_line: logical_line as u32,
                byte_in_line: 0,
                display_row_offset: frac,
            }
        }
        None => {
            let line = row.max(0.0).floor() as u32;
            let frac = (row - line as f32).max(0.0);
            ScrollAnchor {
                logical_line: line,
                byte_in_line: 0,
                display_row_offset: frac,
            }
        }
    }
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
    use crate::app::ui::scrolling::{ContentExtent, ScrollIntent, ViewportMetrics};
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

    #[test]
    fn editor_pixel_offset_round_trips_after_scroll_metrics_are_set() {
        let mut view = EditorViewState::new(7, false);
        view.scroll.set_metrics(ViewportMetrics {
            viewport_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 180.0)),
            row_height: 18.0,
            column_width: 8.0,
            visible_rows: 10,
            visible_columns: 50,
        });
        view.scroll.set_extent(ContentExtent {
            display_rows: 100,
            height: 1800.0,
            max_line_width: 900.0,
        });

        view.set_editor_pixel_offset(egui::vec2(120.0, 360.0));

        assert_eq!(view.editor_pixel_offset(), egui::vec2(120.0, 360.0));
    }

    #[test]
    fn editor_pixel_offset_uses_layout_when_available_for_wrapped_text() {
        use crate::app::domain::RenderedLayout;

        // Build a wrapped layout: 1 logical line that wraps into 3 display rows,
        // followed by 2 unwrapped lines. The naive identity map would place
        // logical_line=2 at row 2, but the layout-aware map should place it at row 3.
        let ctx = egui::Context::default();
        let mut layout = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let text = format!("{}\nshort\nshort", "x".repeat(60));
            let galley = ui.ctx().fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    text,
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    100.0,
                ))
            });
            layout = Some(RenderedLayout::from_galley(galley));
        });
        let layout = layout.expect("layout should be captured");

        // Sanity: there is wrap.
        assert!(layout.row_count() > 3, "expected wrapped layout");
        let first_unwrapped_row = layout
            .display_row_for_logical_line(1)
            .expect("logical line 1 maps to a display row");
        assert!(
            first_unwrapped_row >= 2,
            "logical line 1 should land past the wrapped block"
        );

        let mut view = EditorViewState::new(7, false);
        view.latest_layout = Some(layout);
        view.scroll.set_metrics(ViewportMetrics {
            viewport_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0)),
            row_height: 18.0,
            column_width: 8.0,
            visible_rows: 5,
            visible_columns: 12,
        });
        view.scroll.set_extent(ContentExtent {
            display_rows: 100,
            height: 1800.0,
            max_line_width: 900.0,
        });

        view.set_editor_pixel_offset(egui::vec2(0.0, first_unwrapped_row as f32 * 18.0));
        let anchor = view.scroll.anchor();
        assert_eq!(
            anchor.logical_line, 1,
            "scrolling to row {first_unwrapped_row} should land on logical line 1"
        );
    }

    #[test]
    fn pending_scroll_intents_use_layout_when_available_for_wrapped_text() {
        use crate::app::domain::RenderedLayout;

        let ctx = egui::Context::default();
        let mut layout = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            let text = format!("{}\nshort\nshort", "x".repeat(60));
            let galley = ui.ctx().fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple(
                    text,
                    egui::FontId::monospace(14.0),
                    egui::Color32::WHITE,
                    100.0,
                ))
            });
            layout = Some(RenderedLayout::from_galley(galley));
        });
        let layout = layout.expect("layout should be captured");
        let first_unwrapped_row = layout
            .display_row_for_logical_line(1)
            .expect("logical line 1 maps to a display row");
        assert!(first_unwrapped_row > 1, "expected first line to wrap");

        let mut view = EditorViewState::new(7, false);
        view.latest_layout = Some(layout);
        view.scroll.set_metrics(ViewportMetrics {
            viewport_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0)),
            row_height: 18.0,
            column_width: 8.0,
            visible_rows: 5,
            visible_columns: 12,
        });
        view.scroll.set_extent(ContentExtent {
            display_rows: 100,
            height: 1800.0,
            max_line_width: 900.0,
        });

        view.request_intent(ScrollIntent::Lines(first_unwrapped_row as i32));
        view.apply_pending_scroll_intents();

        assert_eq!(
            view.scroll.anchor().logical_line,
            1,
            "live intent application should resolve rows through the wrapped layout"
        );
    }
}
