use super::*;

impl EditorViewState {
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
        let snapshot = self.latest_display_snapshot.as_ref()?;
        let metrics = self.scroll.metrics();
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
        let document_row = snapshot
            .document_row_for_snapshot_row(snapshot_row)
            .unwrap_or_else(|| row.floor());
        Some(ScrollAnchor::Piece {
            anchor: anchor_id,
            display_row_offset: (row - document_row).max(0.0),
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
}
