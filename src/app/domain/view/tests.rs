use super::{EditorViewState, SearchHighlightState};
use crate::app::domain::{AnchorOwner, BufferState};
use crate::app::ui::scrolling::{ContentExtent, DisplaySnapshot, ScrollAnchor, ViewportMetrics};
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
    DisplaySnapshot::from_galley(&galley.expect("galley"), 10.0)
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
    let mut buffer = BufferState::new("notes.txt".to_owned(), "alpha beta gamma".to_owned(), None);
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
    let mut buffer = BufferState::new("notes.txt".to_owned(), "alpha beta gamma".to_owned(), None);
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
