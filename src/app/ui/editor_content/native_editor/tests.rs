use super::painting::{consume_cursor_reveal, local_cursor};
use super::{
    CharCursor, CursorRange, consumed_page_navigation_direction, editor_content_height,
    editor_desired_size, editor_desired_width, editor_wrap_width, local_range,
    local_search_highlights, request_cursor_reveal_after_input, sync_view_cursor_before_render,
    viewport_text_slice,
};
use crate::app::domain::{BufferState, CursorRevealMode, EditorViewState, SearchHighlightState};
use eframe::egui;

#[test]
fn focused_editor_without_cursor_starts_at_document_beginning() {
    let mut view = EditorViewState::new(1, false);

    sync_view_cursor_before_render(&mut view, true);

    assert_eq!(
        view.cursor_range,
        Some(CursorRange::one(CharCursor::new(0)))
    );
    assert!(view.cursor_reveal_mode().is_some());
}

#[test]
fn viewport_text_slice_extracts_visible_lines_with_overscan() {
    let text = (0..100)
        .map(|line| format!("line-{line}\n"))
        .collect::<String>();
    let buffer = BufferState::new("slice.txt".to_owned(), text, None);
    let row_height = 10.0;
    let viewport = egui::Rect::from_min_size(egui::pos2(0.0, 500.0), egui::vec2(320.0, 40.0));

    let slice = viewport_text_slice(&buffer, viewport, row_height);

    assert!(slice.text.starts_with("line-46\n"));
    assert!(slice.text.contains("line-57\n"));
    assert!(!slice.text.contains("line-45\n"));
    assert!(slice.char_range.start > 0);
}

#[test]
fn local_ranges_are_clipped_to_viewport_slice() {
    assert_eq!(local_range(Some(10..20), 5, 30), Some(5..15));
    assert_eq!(local_range(Some(0..10), 5, 30), Some(0..5));
    assert_eq!(local_range(Some(30..40), 5, 30), None);
}

#[test]
fn local_search_highlights_preserve_active_visible_range() {
    let highlights = SearchHighlightState {
        ranges: vec![0..5, 10..20, 40..50],
        active_range_index: Some(1),
    };

    let local = local_search_highlights(&highlights, 8, 24);

    assert_eq!(local.ranges, vec![2..12]);
    assert_eq!(local.active_range_index, Some(0));
}

#[test]
fn cursor_paint_uses_viewport_local_offset() {
    let local = local_cursor(CharCursor::new(42), 40);

    assert_eq!(local.index, 2);
}

#[test]
fn pending_cursor_range_overrides_missing_native_editor_cursor() {
    let mut view = EditorViewState::new(1, false);
    let pending = CursorRange::one(CharCursor::new(7));
    view.pending_cursor_range = Some(pending);

    sync_view_cursor_before_render(&mut view, true);

    assert_eq!(view.cursor_range, Some(pending));
    assert_eq!(view.pending_cursor_range, None);
    assert!(view.cursor_reveal_mode().is_some());
}

#[test]
fn pending_cursor_sync_preserves_existing_reveal_mode() {
    let mut view = EditorViewState::new(1, false);
    let pending = CursorRange::one(CharCursor::new(7));
    view.pending_cursor_range = Some(pending);
    view.request_cursor_reveal(CursorRevealMode::KeepVisible);

    sync_view_cursor_before_render(&mut view, true);

    assert_eq!(view.cursor_range, Some(pending));
    assert_eq!(
        view.cursor_reveal_mode(),
        Some(CursorRevealMode::KeepVisible)
    );
}

#[test]
fn stable_frame_consumes_scroll_to_cursor_request() {
    let mut view = EditorViewState::new(1, false);
    view.request_cursor_reveal(crate::app::domain::view::CursorRevealMode::KeepVisible);

    consume_cursor_reveal(&mut view, false, true);

    assert!(view.cursor_reveal_mode().is_none());
}

#[test]
fn changed_frame_keeps_scroll_to_cursor_request() {
    let mut view = EditorViewState::new(1, false);
    view.request_cursor_reveal(crate::app::domain::view::CursorRevealMode::KeepVisible);

    consume_cursor_reveal(&mut view, true, true);

    assert!(view.cursor_reveal_mode().is_some());
}

#[test]
fn stable_frame_keeps_scroll_to_cursor_until_cursor_reveal_is_attempted() {
    let mut view = EditorViewState::new(1, false);
    view.request_cursor_reveal(CursorRevealMode::KeepVisible);

    consume_cursor_reveal(&mut view, false, false);

    assert!(view.cursor_reveal_mode().is_some());
}

#[test]
fn same_line_edit_requests_horizontal_only_cursor_reveal() {
    let buffer = BufferState::new("test.txt".to_owned(), "alpha!".to_owned(), None);
    let mut view = EditorViewState::new(buffer.id, false);
    let previous = CursorRange::one(CharCursor::new(5));
    view.cursor_range = Some(CursorRange::one(CharCursor::new(6)));

    request_cursor_reveal_after_input(&buffer, &mut view, Some(previous), Some(0), true);

    assert_eq!(
        view.cursor_reveal_mode(),
        Some(CursorRevealMode::KeepHorizontalVisible)
    );
}

#[test]
fn newline_edit_keeps_vertical_cursor_reveal() {
    let buffer = BufferState::new("test.txt".to_owned(), "alpha\n".to_owned(), None);
    let mut view = EditorViewState::new(buffer.id, false);
    let previous = CursorRange::one(CharCursor::new(5));
    view.cursor_range = Some(CursorRange::one(CharCursor::new(6)));

    request_cursor_reveal_after_input(&buffer, &mut view, Some(previous), Some(0), true);

    assert_eq!(
        view.cursor_reveal_mode(),
        Some(CursorRevealMode::KeepVisible)
    );
}

#[test]
fn editor_desired_size_does_not_add_extra_trailing_scroll_space() {
    let desired = editor_desired_size_for_test(400.0, 200.0, 400.0, 400.0);

    assert_eq!(desired, Some(egui::vec2(400.0, 400.0)));
}

#[test]
fn editor_content_height_tracks_wrapped_visual_rows() {
    let height = editor_content_height_for_test(80.0, "W".repeat(200).as_str());

    assert!(height.is_some_and(|(height, row_height)| height > row_height * 2.0));
}

#[test]
fn editor_desired_width_uses_wrap_point_when_wrapping() {
    let width = editor_desired_width_for_test(400.0, "W".repeat(200).as_str(), true);

    assert_eq!(width, Some(400.0));
}

#[test]
fn editor_wrap_width_uses_viewport_when_child_ui_is_unbounded() {
    let ctx = egui::Context::default();
    let mut width = None;
    let _ = ctx.run_ui(Default::default(), |ui| {
        ui.set_width(f32::INFINITY);
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 200.0));
        width = Some(editor_wrap_width(ui, true, Some(viewport)));
    });

    assert_eq!(width, Some(320.0));
}

#[test]
fn editor_desired_width_uses_longest_line_without_wrap() {
    let width = editor_desired_width_for_test(400.0, "W".repeat(200).as_str(), false);

    assert!(width.is_some_and(|width| width > 400.0));
}

fn editor_desired_size_for_test(
    available_width: f32,
    available_height: f32,
    desired_width: f32,
    desired_height: f32,
) -> Option<egui::Vec2> {
    let ctx = egui::Context::default();
    let mut desired = None;
    let _ = ctx.run_ui(Default::default(), |ui| {
        ui.set_width(available_width);
        ui.set_height(available_height);
        desired = Some(editor_desired_size(ui, desired_width, desired_height));
    });
    desired
}

fn editor_content_height_for_test(wrap_width: f32, text: &str) -> Option<(f32, f32)> {
    let ctx = egui::Context::default();
    let mut height = None;
    let _ = ctx.run_ui(Default::default(), |ui| {
        let font_id = egui::FontId::monospace(14.0);
        let row_height = ui.fonts_mut(|fonts| fonts.row_height(&font_id));
        let galley = ui.ctx().fonts_mut(|fonts| {
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = wrap_width;
            job.append(
                text,
                0.0,
                egui::TextFormat {
                    font_id,
                    ..Default::default()
                },
            );
            fonts.layout_job(job)
        });
        height = Some((editor_content_height(&galley, row_height), row_height));
    });
    height
}

fn editor_desired_width_for_test(available_width: f32, text: &str, word_wrap: bool) -> Option<f32> {
    let ctx = egui::Context::default();
    let mut desired = None;
    let _ = ctx.run_ui(Default::default(), |ui| {
        ui.set_width(available_width);
        let galley = ui.ctx().fonts_mut(|fonts| {
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = f32::INFINITY;
            job.append(
                text,
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(14.0),
                    ..Default::default()
                },
            );
            fonts.layout_job(job)
        });
        desired = Some(editor_desired_width(ui, &galley, word_wrap, None));
    });
    desired
}

#[test]
fn page_navigation_emits_pages_intent_with_signed_direction() {
    let ctx = egui::Context::default();
    let mut direction = None;
    let _ = ctx.run_ui(Default::default(), |ui| {
        ui.input_mut(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::PageDown,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        });

        direction = consumed_page_navigation_direction(ui);
    });

    assert_eq!(direction, Some(1));

    let mut direction = None;
    let _ = ctx.run_ui(Default::default(), |ui| {
        ui.input_mut(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::PageUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        });

        direction = consumed_page_navigation_direction(ui);
    });

    assert_eq!(direction, Some(-1));
}
