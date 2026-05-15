use crate::app::domain::{BufferState, EditorViewState, ViewId, WorkspaceTab};
use crate::app::ui::editor_content::extent;
use crate::app::ui::scrolling::{DisplaySnapshot, ScrollAnchor};
use crate::app::ui::{scrolling, widget_ids};
use eframe::egui;

pub(super) fn recover_unresolved_piece_anchor(
    ui: &egui::Ui,
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    scroll_id: egui::Id,
    snapshot_fallback: Option<&DisplaySnapshot>,
) {
    let preserved_offset = scrolling::ScrollState::load(ui, scroll_id).offset;
    let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) else {
        return;
    };
    let ScrollAnchor::Piece { anchor, .. } = view.scroll.anchor() else {
        return;
    };
    let snapshot = view.latest_display_snapshot.as_ref().or(snapshot_fallback);
    let resolved_char_offset = buffer.document().piece_tree().anchor_position(anchor);
    let unresolved = match resolved_char_offset {
        Some(char_offset) => snapshot
            .is_some_and(|snapshot| snapshot.row_for_char_offset(char_offset as u32).is_none()),
        None => true,
    };
    if !unresolved {
        return;
    }

    if !release_tracked_piece_anchor(view, buffer) {
        view.scroll.replace_anchor(ScrollAnchor::TOP);
    }
    view.set_editor_pixel_offset(preserved_offset);
}

pub(super) fn sync_local_scroll_state(ui: &egui::Ui, scroll_id: egui::Id, offset: egui::Vec2) {
    sync_editor_scroll_state(ui, scroll_id, offset);
    let mut local_state = scrolling::ScrollState::load(ui, scroll_id);
    local_state.offset = offset;
    local_state.store(ui, scroll_id);
}

pub(super) fn virtual_editor_content_height(
    tab: &WorkspaceTab,
    view_id: ViewId,
    virtual_row_height: f32,
    viewport_height: f32,
    previous_snapshot: Option<&DisplaySnapshot>,
) -> f32 {
    tab.buffer_for_view(view_id)
        .map(|buffer| {
            extent::scroll_content_height(
                buffer.line_count.max(1),
                virtual_row_height,
                viewport_height,
                previous_snapshot,
            )
        })
        .unwrap_or_default()
}

pub(super) fn publish_scroll_manager_metrics(
    view: &mut EditorViewState,
    viewport_rect: egui::Rect,
    row_height: f32,
    content_size: egui::Vec2,
) {
    let visible_rows = if row_height > 0.0 {
        (viewport_rect.height() / row_height).ceil().max(1.0) as u32
    } else {
        1
    };
    view.scroll.set_metrics(scrolling::ViewportMetrics {
        viewport_rect,
        row_height,
        column_width: row_height * 0.5,
        visible_rows,
        visible_columns: 0,
    });
    let display_rows = if row_height > 0.0 {
        (content_size.y / row_height).ceil().max(0.0) as u32
    } else {
        0
    };
    view.scroll.set_extent(scrolling::ContentExtent {
        display_rows,
        height: content_size.y,
        max_line_width: content_size.x,
    });
}

pub(super) fn resolved_scroll_offset_for_view(
    tab: &WorkspaceTab,
    view_id: ViewId,
    previous_snapshot: Option<&DisplaySnapshot>,
) -> egui::Vec2 {
    tab.view(view_id)
        .and_then(|view| {
            tab.buffer_for_view(view_id)
                .map(|buffer| editor_pixel_offset_resolved(view, buffer, previous_snapshot))
        })
        .unwrap_or_default()
}

pub(super) fn drain_pending_scroll_intents(
    view: &mut EditorViewState,
    buffer: &mut BufferState,
    snapshot_fallback: Option<&DisplaySnapshot>,
    fallback_pixel_offset: Option<egui::Vec2>,
) {
    if view.pending_intents.is_empty() {
        return;
    }
    if pending_intents_include_reveal(view)
        && let Some(offset) = fallback_pixel_offset
        && current_scroll_anchor_unresolved(view, buffer, snapshot_fallback)
    {
        release_piece_anchor_and_restore_pixel_offset(view, buffer, offset);
    }
    let intents = std::mem::take(&mut view.pending_intents);
    let snapshot = view
        .latest_display_snapshot
        .as_ref()
        .or(snapshot_fallback)
        .cloned();
    let resolve = |id| buffer.document().piece_tree().anchor_position(id);
    let anchor_to_row = scrolling::display_aware_anchor_to_row(snapshot.as_ref(), resolve);
    for intent in intents {
        view.scroll
            .apply_intent(intent, &anchor_to_row, scrolling::naive_row_to_anchor);
    }
}

pub(super) fn sync_programmatic_scroll_offset(
    ui: &egui::Ui,
    view: &mut EditorViewState,
    buffer: &mut BufferState,
    scroll_id: egui::Id,
    viewport_rect: egui::Rect,
) {
    let content_size = egui::vec2(
        view.scroll.extent().max_line_width,
        view.scroll.extent().height,
    );
    let offset = super::scroll_input::resolve_editor_scroll_offset_override(
        content_size,
        viewport_rect.size(),
        Some(view.editor_pixel_offset_resolved(buffer)),
        None,
        None,
    )
    .unwrap_or_default();
    view.set_editor_pixel_offset_resolved(buffer, offset);
    sync_local_scroll_state(ui, scroll_id, offset);
    ui.ctx().request_repaint();
}

pub(super) fn editor_scroll_content_size(
    content_size: egui::Vec2,
    virtual_content_height: f32,
    wrapped_viewport_width: Option<f32>,
) -> egui::Vec2 {
    egui::vec2(
        wrapped_viewport_width
            .filter(|width| width.is_finite() && *width > 0.0)
            .unwrap_or(content_size.x),
        content_size.y.max(virtual_content_height.max(0.0)),
    )
}

fn editor_pixel_offset_resolved(
    view: &EditorViewState,
    buffer: &BufferState,
    snapshot_fallback: Option<&DisplaySnapshot>,
) -> egui::Vec2 {
    let snapshot = view.latest_display_snapshot.as_ref().or(snapshot_fallback);
    let resolve = |id| buffer.document().piece_tree().anchor_position(id);
    let anchor_to_row = scrolling::display_aware_anchor_to_row(snapshot, resolve);
    let y = view.scroll.pixel_offset_y(anchor_to_row);
    egui::vec2(view.scroll.horizontal_px(), y)
}

fn pending_intents_include_reveal(view: &EditorViewState) -> bool {
    view.pending_intents
        .iter()
        .any(|intent| matches!(intent, scrolling::ScrollIntent::Reveal { .. }))
}

fn current_scroll_anchor_unresolved(
    view: &EditorViewState,
    buffer: &BufferState,
    snapshot_fallback: Option<&DisplaySnapshot>,
) -> bool {
    let Some(anchor) = view.scroll.anchor().piece_anchor() else {
        return false;
    };
    let Some(snapshot) = view.latest_display_snapshot.as_ref().or(snapshot_fallback) else {
        return false;
    };
    let Some(char_offset) = buffer.document().piece_tree().anchor_position(anchor) else {
        return true;
    };
    snapshot.row_for_char_offset(char_offset as u32).is_none()
}

fn release_piece_anchor_and_restore_pixel_offset(
    view: &mut EditorViewState,
    buffer: &mut BufferState,
    offset: egui::Vec2,
) {
    view.set_editor_pixel_offset(offset);
    release_tracked_piece_anchor(view, buffer);
}

fn release_tracked_piece_anchor(view: &mut EditorViewState, buffer: &mut BufferState) -> bool {
    let Some(anchor) = view.take_piece_anchor_for_release() else {
        return false;
    };
    buffer
        .document_mut()
        .piece_tree_mut()
        .release_anchor(anchor);
    true
}

fn sync_editor_scroll_state(ui: &egui::Ui, scroll_id: egui::Id, offset: egui::Vec2) {
    let persistent_id = widget_ids::local(ui, ("editor_scroll_state", scroll_id));
    let mut state = egui::scroll_area::State::load(ui.ctx(), persistent_id).unwrap_or_default();
    if state.offset != offset {
        state.offset = offset;
        state.store(ui.ctx(), persistent_id);
    }
}

#[cfg(test)]
mod tests {
    use super::editor_scroll_content_size;
    use crate::app::domain::{BufferState, WorkspaceTab};
    use crate::app::ui::editor_content::extent;
    use eframe::egui;

    #[test]
    fn wrapped_scroll_content_width_tracks_viewport() {
        let size = editor_scroll_content_size(egui::vec2(900.0, 200.0), 300.0, Some(640.0));

        assert_eq!(size, egui::vec2(640.0, 300.0));
    }

    #[test]
    fn unwrapped_scroll_content_width_preserves_measured_extent() {
        let size = editor_scroll_content_size(egui::vec2(900.0, 200.0), 300.0, None);

        assert_eq!(size, egui::vec2(900.0, 300.0));
    }

    #[test]
    fn eof_tail_does_not_create_blank_scroll_page() {
        assert_eq!(extent::eof_tail_height(600.0, 20.0), 0.0);
    }

    #[test]
    fn virtual_content_height_excludes_blank_eof_page() {
        let text = (0..20)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let tab = WorkspaceTab::new(BufferState::new("sample.txt".to_owned(), text, None));

        let height = super::virtual_editor_content_height(
            &tab,
            tab.layout.active_view_id,
            20.0,
            200.0,
            None,
        );

        assert_eq!(height, 400.0);
    }
}
