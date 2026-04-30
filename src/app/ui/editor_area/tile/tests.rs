use super::{
    clamp_scroll_offset, editor_pixel_offset_resolved, editor_scroll_id, max_scroll_offset,
    recover_unresolved_piece_anchor, scroll_offset_from_wheel_delta, selection_edge_drag_velocity,
    suppress_view_reveals_for_selection_drag, virtual_editor_content_height,
};
use crate::app::domain::{AnchorBias, BufferState, EditorViewState, WorkspaceTab};
use crate::app::ui::scrolling::{
    ContentExtent, DisplayRow, DisplaySnapshot, ScrollAlign, ScrollAnchor, ScrollIntent,
    ScrollState, ViewportMetrics,
};
use eframe::egui;

fn galley_for(text: &str) -> std::sync::Arc<egui::Galley> {
    galley_for_width(text, f32::INFINITY)
}

fn galley_for_width(text: &str, wrap_width: f32) -> std::sync::Arc<egui::Galley> {
    let ctx = egui::Context::default();
    let mut galley = None;
    let _ = ctx.run_ui(Default::default(), |ui| {
        galley = Some(ui.fonts_mut(|fonts| {
            fonts.layout_job(egui::text::LayoutJob::simple(
                text.to_owned(),
                egui::FontId::monospace(14.0),
                egui::Color32::WHITE,
                wrap_width,
            ))
        }));
    });
    galley.expect("galley")
}

fn snapshot_for(text: &str, wrap_width: f32) -> DisplaySnapshot {
    DisplaySnapshot::from_galley(&galley_for_width(text, wrap_width), 10.0)
}

fn sliced_snapshot_for_lines(text: &str, start_line: usize, end_line: usize) -> DisplaySnapshot {
    let start_char = char_offset_for_line(text, start_line);
    let end_char = char_offset_for_line(text, end_line);
    let slice = text
        .chars()
        .skip(start_char)
        .take(end_char.saturating_sub(start_char))
        .collect::<String>();
    DisplaySnapshot::from_galley_with_base(
        &galley_for_width(&slice, f32::INFINITY),
        10.0,
        start_char,
        start_line,
    )
}

fn numbered_lines(count: usize) -> String {
    (0..count).map(|line| format!("line {line:03}\n")).collect()
}

fn char_offset_for_line(text: &str, line_index: usize) -> usize {
    text.lines()
        .take(line_index)
        .map(|line| line.chars().count() + 1)
        .sum()
}

fn set_view_geometry(view: &mut EditorViewState, snapshot: &DisplaySnapshot) {
    view.scroll.set_metrics(ViewportMetrics {
        viewport_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 40.0)),
        row_height: 10.0,
        column_width: 5.0,
        visible_rows: 4,
        visible_columns: 40,
    });
    view.scroll.set_extent(ContentExtent {
        display_rows: snapshot.row_count(),
        height: snapshot.content_height(),
        max_line_width: snapshot.max_line_width(),
    });
}

fn set_full_document_geometry(view: &mut EditorViewState, rows: u32) {
    view.scroll.set_metrics(ViewportMetrics {
        viewport_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 40.0)),
        row_height: 10.0,
        column_width: 5.0,
        visible_rows: 4,
        visible_columns: 40,
    });
    view.scroll.set_extent(ContentExtent {
        display_rows: rows,
        height: rows as f32 * 10.0,
        max_line_width: 200.0,
    });
}

fn set_piece_scroll_anchor_at_row(
    view: &mut EditorViewState,
    buffer: &mut BufferState,
    snapshot: DisplaySnapshot,
    row: u32,
) {
    set_view_geometry(view, &snapshot);
    view.latest_display_snapshot = Some(snapshot);
    view.set_editor_pixel_offset(egui::vec2(0.0, row as f32 * 10.0));
    view.upgrade_scroll_anchor_to_piece(buffer);
}

fn rebuild_view_snapshot(view: &mut EditorViewState, text: &str, wrap_width: f32) {
    let snapshot = snapshot_for(text, wrap_width);
    set_view_geometry(view, &snapshot);
    view.latest_display_snapshot = Some(snapshot);
}

fn top_row_text(view: &EditorViewState, buffer: &BufferState) -> String {
    let offset = editor_pixel_offset_resolved(view, buffer, None);
    let snapshot = view.latest_display_snapshot.as_ref().expect("snapshot");
    let row = (offset.y / view.scroll.metrics().row_height)
        .floor()
        .max(0.0);
    let range = snapshot
        .row_for_document_row(row)
        .and_then(|row| snapshot.row_char_range(row))
        .expect("row range");
    buffer
        .document()
        .piece_tree()
        .extract_range(range.start as usize..range.end as usize)
        .trim_end_matches('\n')
        .to_owned()
}

#[test]
fn editor_scroll_id_is_scoped_to_the_view() {
    assert_eq!(editor_scroll_id(7), editor_scroll_id(7));
    assert_ne!(editor_scroll_id(7), editor_scroll_id(8));
}

#[test]
fn piece_anchor_pixel_offset_uses_previous_snapshot_fallback() {
    let text = "zero\none\ntwo\nthree\nfour";
    let mut buffer = BufferState::new("notes.txt".to_owned(), text.to_owned(), None);
    let anchor = buffer
        .document_mut()
        .piece_tree_mut()
        .create_anchor(text.find("three").expect("line start"), AnchorBias::Left);
    let snapshot = DisplaySnapshot::from_galley(&galley_for(text), 10.0);
    let mut view = EditorViewState::new(buffer.id, false);
    view.scroll.set_metrics(ViewportMetrics {
        viewport_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 40.0)),
        row_height: 10.0,
        column_width: 5.0,
        visible_rows: 4,
        visible_columns: 40,
    });
    view.scroll.replace_anchor(ScrollAnchor::Piece {
        anchor,
        display_row_offset: 0.25,
    });

    assert_eq!(editor_pixel_offset_resolved(&view, &buffer, None).y, 2.5);
    assert_eq!(
        editor_pixel_offset_resolved(&view, &buffer, Some(&snapshot)).y,
        32.5
    );
}

#[test]
fn sliced_snapshot_piece_anchor_resolves_to_document_pixel_offset_near_eof() {
    let text = numbered_lines(120);
    let mut buffer = BufferState::new("tail.txt".to_owned(), text.clone(), None);
    let snapshot = sliced_snapshot_for_lines(&text, 116, 120);
    let anchor = buffer
        .document_mut()
        .piece_tree_mut()
        .create_anchor(char_offset_for_line(&text, 118), AnchorBias::Left);
    let mut view = EditorViewState::new(buffer.id, false);
    set_full_document_geometry(&mut view, 120);
    view.latest_display_snapshot = Some(snapshot);
    view.scroll.replace_anchor(ScrollAnchor::Piece {
        anchor,
        display_row_offset: 0.0,
    });

    assert_eq!(editor_pixel_offset_resolved(&view, &buffer, None).y, 1180.0);
}

#[test]
fn bottom_scroll_offset_seeds_piece_anchor_from_sliced_snapshot() {
    let text = numbered_lines(120);
    let mut buffer = BufferState::new("tail.txt".to_owned(), text.clone(), None);
    let snapshot = sliced_snapshot_for_lines(&text, 116, 120);
    let mut view = EditorViewState::new(buffer.id, false);
    set_full_document_geometry(&mut view, 120);
    view.latest_display_snapshot = Some(snapshot);

    view.set_editor_pixel_offset_resolved(&mut buffer, egui::vec2(0.0, 1160.0));

    assert!(matches!(view.scroll.anchor(), ScrollAnchor::Piece { .. }));
    assert_eq!(editor_pixel_offset_resolved(&view, &buffer, None).y, 1160.0);
    assert_eq!(top_row_text(&view, &buffer), "line 116");
}

#[test]
fn unresolved_piece_anchor_recovers_from_local_scroll_state() {
    let text = numbered_lines(120);
    let snapshot = snapshot_for(&text, f32::INFINITY);
    let buffer = BufferState::new("notes.txt".to_owned(), text.clone(), None);
    let mut tab = WorkspaceTab::new(buffer);
    let view_id = tab.active_view_id;
    let scroll_id = editor_scroll_id(view_id);

    {
        let (buffer, view) = tab.buffer_and_view_mut(view_id).expect("active view");
        set_piece_scroll_anchor_at_row(view, buffer, snapshot.clone(), 60);
        let anchor = view.scroll.anchor().piece_anchor().expect("piece anchor");
        buffer
            .document_mut()
            .piece_tree_mut()
            .release_anchor(anchor);
        assert_eq!(buffer.document().piece_tree().anchor_position(anchor), None);
    }

    let ctx = egui::Context::default();
    let _ = ctx.run_ui(Default::default(), |ui| {
        let state = ScrollState {
            offset: egui::vec2(0.0, 600.0),
            ..Default::default()
        };
        state.store(ui, scroll_id);

        recover_unresolved_piece_anchor(ui, &mut tab, view_id, scroll_id, Some(&snapshot));
    });

    let (buffer, view) = tab.buffer_and_view_mut(view_id).expect("active view");
    assert_eq!(view.scroll.anchor().logical_line(), Some(60));
    assert_eq!(
        editor_pixel_offset_resolved(view, buffer, Some(&snapshot)).y,
        600.0
    );

    view.upgrade_scroll_anchor_to_piece(buffer);

    assert!(matches!(view.scroll.anchor(), ScrollAnchor::Piece { .. }));
    assert_eq!(top_row_text(view, buffer), "line 060");
}

#[test]
fn piece_anchor_keeps_top_content_after_insert_above_viewport() {
    let text = numbered_lines(120);
    let mut buffer = BufferState::new("notes.txt".to_owned(), text.clone(), None);
    let snapshot = snapshot_for(&text, f32::INFINITY);
    let mut view = EditorViewState::new(buffer.id, false);

    set_piece_scroll_anchor_at_row(&mut view, &mut buffer, snapshot, 60);
    buffer
        .document_mut()
        .insert_direct(0, "new 000\nnew 001\nnew 002\nnew 003\nnew 004\n");
    rebuild_view_snapshot(&mut view, &buffer.text(), f32::INFINITY);

    assert_eq!(top_row_text(&view, &buffer), "line 060");
}

#[test]
fn piece_anchor_keeps_top_content_after_delete_above_viewport() {
    let text = numbered_lines(120);
    let mut buffer = BufferState::new("notes.txt".to_owned(), text.clone(), None);
    let snapshot = snapshot_for(&text, f32::INFINITY);
    let mut view = EditorViewState::new(buffer.id, false);
    let delete_start = char_offset_for_line(&text, 10);
    let delete_end = char_offset_for_line(&text, 20);

    set_piece_scroll_anchor_at_row(&mut view, &mut buffer, snapshot, 60);
    buffer
        .document_mut()
        .delete_char_range_direct(delete_start..delete_end);
    rebuild_view_snapshot(&mut view, &buffer.text(), f32::INFINITY);

    assert_eq!(top_row_text(&view, &buffer), "line 060");
}

#[test]
fn top_of_viewport_insert_uses_left_bias_semantics() {
    let text = numbered_lines(40);
    let mut buffer = BufferState::new("notes.txt".to_owned(), text.clone(), None);
    let snapshot = snapshot_for(&text, f32::INFINITY);
    let mut view = EditorViewState::new(buffer.id, false);
    let insert_at = char_offset_for_line(&text, 20);

    set_piece_scroll_anchor_at_row(&mut view, &mut buffer, snapshot, 20);
    buffer
        .document_mut()
        .insert_direct(insert_at, "inserted at top\n");
    rebuild_view_snapshot(&mut view, &buffer.text(), f32::INFINITY);

    assert_eq!(top_row_text(&view, &buffer), "inserted at top");
}

#[test]
fn piece_anchor_keeps_wrapped_top_row_after_insert_above_viewport() {
    let line = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu\n";
    let text = line.repeat(40);
    let mut buffer = BufferState::new("wrapped.txt".to_owned(), text.clone(), None);
    let snapshot = snapshot_for(&text, 120.0);
    let mut view = EditorViewState::new(buffer.id, false);
    let anchored_row = 18;
    let original_top = snapshot
        .row_char_range(DisplayRow(anchored_row))
        .map(|range| {
            text.chars()
                .skip(range.start as usize)
                .take((range.end - range.start) as usize)
                .collect::<String>()
        })
        .expect("row text")
        .trim_end_matches('\n')
        .to_owned();

    set_piece_scroll_anchor_at_row(&mut view, &mut buffer, snapshot, anchored_row);
    buffer
        .document_mut()
        .insert_direct(0, "short inserted line\nshort inserted line\n");
    rebuild_view_snapshot(&mut view, &buffer.text(), 120.0);

    assert_eq!(top_row_text(&view, &buffer), original_top);
}

#[test]
fn split_views_keep_independent_piece_anchors_with_different_wrap_widths() {
    let line = "one two three four five six seven eight nine ten eleven twelve\n";
    let text = line.repeat(80);
    let mut buffer = BufferState::new("split.txt".to_owned(), text.clone(), None);
    let narrow_snapshot = snapshot_for(&text, 110.0);
    let wide_snapshot = snapshot_for(&text, 320.0);
    let mut narrow_view = EditorViewState::new(buffer.id, false);
    let mut wide_view = EditorViewState::new(buffer.id, false);
    let narrow_row = 24;
    let wide_row = 18;
    let narrow_top = narrow_snapshot
        .row_char_range(DisplayRow(narrow_row))
        .map(|range| {
            text.chars()
                .skip(range.start as usize)
                .take((range.end - range.start) as usize)
                .collect::<String>()
        })
        .expect("narrow row text")
        .trim_end_matches('\n')
        .to_owned();
    let wide_top = wide_snapshot
        .row_char_range(DisplayRow(wide_row))
        .map(|range| {
            text.chars()
                .skip(range.start as usize)
                .take((range.end - range.start) as usize)
                .collect::<String>()
        })
        .expect("wide row text")
        .trim_end_matches('\n')
        .to_owned();

    set_piece_scroll_anchor_at_row(&mut narrow_view, &mut buffer, narrow_snapshot, narrow_row);
    set_piece_scroll_anchor_at_row(&mut wide_view, &mut buffer, wide_snapshot, wide_row);
    buffer
        .document_mut()
        .insert_direct(0, "preface\npreface\npreface\n");
    rebuild_view_snapshot(&mut narrow_view, &buffer.text(), 110.0);
    rebuild_view_snapshot(&mut wide_view, &buffer.text(), 320.0);

    assert_eq!(top_row_text(&narrow_view, &buffer), narrow_top);
    assert_eq!(top_row_text(&wide_view, &buffer), wide_top);
}

#[test]
fn near_eof_piece_anchor_remains_resolvable_after_delete_above_viewport() {
    let text = numbered_lines(90);
    let mut buffer = BufferState::new("tail.txt".to_owned(), text.clone(), None);
    let snapshot = snapshot_for(&text, f32::INFINITY);
    let mut view = EditorViewState::new(buffer.id, false);
    let delete_start = char_offset_for_line(&text, 5);
    let delete_end = char_offset_for_line(&text, 25);

    set_piece_scroll_anchor_at_row(&mut view, &mut buffer, snapshot, 88);
    buffer
        .document_mut()
        .delete_char_range_direct(delete_start..delete_end);
    rebuild_view_snapshot(&mut view, &buffer.text(), f32::INFINITY);

    assert_eq!(top_row_text(&view, &buffer), "line 088");
}

#[test]
fn wheel_delta_requests_explicit_scroll_offset() {
    assert_eq!(
        scroll_offset_from_wheel_delta(egui::vec2(12.0, 90.0), egui::vec2(4.0, -18.0)),
        Some(egui::vec2(8.0, 108.0))
    );
    assert_eq!(
        scroll_offset_from_wheel_delta(egui::vec2(0.0, 10.0), egui::vec2(0.0, 20.0)),
        Some(egui::vec2(0.0, 0.0))
    );
}

// `editor_scroll_source_disables_builtin_drag_scrolling` was removed —
// the editor no longer wraps `egui::ScrollArea`. The local
// `scrolling::ScrollSource` and its `EDITOR` preset are unit-tested in
// `app::ui::scrolling`.

#[test]
fn selection_edge_drag_velocity_is_symmetric_at_top_and_bottom() {
    let viewport = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 200.0));
    let row_height = 18.0;
    let mid_x = 100.0;
    let edge_offset = 1.5 * row_height - 1.0;

    let top = selection_edge_drag_velocity(
        viewport,
        egui::pos2(mid_x, viewport.top() + edge_offset),
        row_height,
    );
    let bottom = selection_edge_drag_velocity(
        viewport,
        egui::pos2(mid_x, viewport.bottom() - edge_offset),
        row_height,
    );

    assert!(top.y < 0.0, "top should scroll up, got {top:?}");
    assert!(bottom.y > 0.0, "bottom should scroll down, got {bottom:?}");
    assert!(
        (top.y + bottom.y).abs() < 1e-3,
        "top/bottom magnitudes should match: {top:?} vs {bottom:?}"
    );
}

#[test]
fn selection_edge_drag_velocity_pushes_down_near_bottom_edge() {
    assert!(
        selection_edge_drag_velocity(
            egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 120.0)),
            egui::pos2(100.0, 150.0),
            18.0,
        )
        .y > 0.0
    );
}

#[test]
fn selection_edge_drag_velocity_is_zero_away_from_edges() {
    assert_eq!(
        selection_edge_drag_velocity(
            egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 120.0)),
            egui::pos2(100.0, 80.0),
            18.0,
        ),
        egui::Vec2::ZERO
    );
}

#[test]
fn selection_edge_drag_velocity_starts_at_edge_activation_zone() {
    let viewport = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 120.0));
    let row_height = 18.0;
    let edge_extent = 24.0_f32.max(row_height * 1.5);

    assert_eq!(
        selection_edge_drag_velocity(
            viewport,
            egui::pos2(100.0, viewport.bottom() - edge_extent),
            row_height,
        ),
        egui::Vec2::ZERO
    );
    assert!(
        selection_edge_drag_velocity(
            viewport,
            egui::pos2(100.0, viewport.bottom() - edge_extent + 1.0),
            row_height,
        )
        .y >= row_height * 8.0
    );
}

#[test]
fn selection_edge_drag_velocity_accelerates_outside_viewport() {
    let viewport = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 120.0));
    let row_height = 18.0;
    let near_edge = selection_edge_drag_velocity(
        viewport,
        egui::pos2(100.0, viewport.bottom() - 8.0),
        row_height,
    );
    let outside = selection_edge_drag_velocity(
        viewport,
        egui::pos2(100.0, viewport.bottom() + row_height * 3.0),
        row_height,
    );

    assert!(
        outside.y > near_edge.y,
        "outside={outside:?}, near_edge={near_edge:?}"
    );
    assert!(
        outside.y >= row_height * 30.0,
        "outside should move briskly enough for large selections: {outside:?}"
    );
}

#[test]
fn selection_drag_suppresses_pending_reveal_scroll_intents() {
    let mut view = EditorViewState::new(1, false);
    view.request_cursor_reveal(crate::app::domain::CursorRevealMode::KeepVisible);
    view.request_intent(ScrollIntent::Reveal {
        rect: egui::Rect::from_min_size(egui::pos2(0.0, 200.0), egui::vec2(8.0, 10.0)),
        align_y: Some(ScrollAlign::NearestWithMargin(24.0)),
        align_x: None,
    });
    view.request_intent(ScrollIntent::Lines(1));

    suppress_view_reveals_for_selection_drag(&mut view);

    assert_eq!(view.cursor_reveal_mode(), None);
    assert_eq!(view.pending_intents.len(), 1);
    assert!(matches!(view.pending_intents[0], ScrollIntent::Lines(1)));
}

#[test]
fn clamp_scroll_offset_limits_east_and_south_to_content_bounds() {
    assert_eq!(
        clamp_scroll_offset(
            egui::vec2(280.0, 220.0),
            egui::vec2(320.0, 260.0),
            egui::vec2(120.0, 100.0),
        ),
        egui::vec2(200.0, 160.0)
    );
    assert_eq!(
        max_scroll_offset(egui::vec2(320.0, 260.0), egui::vec2(120.0, 100.0)),
        egui::vec2(200.0, 160.0)
    );
}

#[test]
fn virtual_content_height_includes_eof_tail_for_final_line_top_scroll() {
    let buffer = BufferState::new("tail.txt".to_owned(), numbered_lines(120), None);
    let line_count = buffer.line_count.max(1) as f32;
    let tab = WorkspaceTab::new(buffer);
    let content_height = virtual_editor_content_height(&tab, tab.active_view_id, 10.0, 40.0);

    assert_eq!(content_height, line_count * 10.0 + 30.0);
    assert_eq!(
        max_scroll_offset(egui::vec2(200.0, content_height), egui::vec2(200.0, 40.0)).y,
        (line_count - 1.0) * 10.0
    );
}

// `duplicated_views_can_track_independent_scroll_offsets` was removed as
// part of the scrolling rebuild — it asserted the old pixel-offset API.
// The replacement coverage will be added in Phase 6 against the
// `ScrollManager`-based view state.
