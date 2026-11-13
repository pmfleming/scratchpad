use super::geometry::split_preview_spec;
use super::preview::{build_preview_lines, calculate_floating_tile_rect, elide_preview_line};
use super::{SplitAxis, SplitPreviewOverlay, paint_split_preview};
use crate::app::app_state::test_support::render_once;
use eframe::egui;

#[test]
fn drag_direction_picks_axis_side_and_snapped_clamped_ratio() {
    let tile = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
    let start = egui::pos2(200.0, 100.0);
    let cases = [
        ((210.0, 105.0), None),
        ((300.0, 100.0), Some((SplitAxis::Vertical, false, 0.75))),
        ((50.0, 100.0), Some((SplitAxis::Vertical, true, 0.2))),
        ((215.0, 100.0), Some((SplitAxis::Vertical, false, 0.5))),
        ((200.0, 190.0), Some((SplitAxis::Horizontal, false, 0.8))),
        ((200.0, 20.0), Some((SplitAxis::Horizontal, true, 0.2))),
    ];
    for ((x, y), expected) in cases {
        assert_eq!(
            split_preview_spec(tile, start, egui::pos2(x, y)),
            expected,
            "drag to ({x}, {y})"
        );
    }
}

#[test]
fn preview_text_is_limited_tab_expanded_and_elided() {
    assert_eq!(build_preview_lines(""), ["Untitled"]);
    assert_eq!(
        build_preview_lines("a\tb\n2\n3\n4\n5"),
        ["a    b", "2", "3", "4"]
    );
    assert_eq!(elide_preview_line("short", 70.0), "short");
    assert_eq!(elide_preview_line("abcdefghijkl", 70.0), "abcdefghi…");
    assert_eq!(elide_preview_line("abcdefghijkl", 0.0), "abcdefg…");
}

fn overlay(
    axis: Option<SplitAxis>,
    pointer: (f32, f32),
    anchor: (f32, f32),
) -> SplitPreviewOverlay {
    SplitPreviewOverlay {
        axis,
        new_view_first: axis == Some(SplitAxis::Vertical),
        ratio: 0.5,
        pointer_pos: egui::pos2(pointer.0, pointer.1),
        tile_rect: egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(300.0, 200.0)),
        handle_anchor: egui::pos2(anchor.0, anchor.1),
        title: "notes.txt".to_owned(),
        preview_lines: build_preview_lines("first line\nsecond line"),
    }
}

#[test]
fn floating_preview_hangs_left_of_its_anchor_and_stays_on_screen() {
    let max_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0));
    let cases = [
        ((100.0, 200.0), (300.0, 30.0), [100.0, 30.0, 300.0, 200.0]),
        ((5000.0, -50.0), (300.0, 30.0), [268.0, 30.0, 300.0, 62.0]),
        ((5000.0, -50.0), (-10.0, 500.0), [0.0, 268.0, 32.0, 300.0]),
    ];
    for (pointer, anchor, [left, top, right, bottom]) in cases {
        let rect = calculate_floating_tile_rect(&overlay(None, pointer, anchor), max_rect);
        assert_eq!(
            rect,
            egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom)),
            "pointer {pointer:?}, anchor {anchor:?}"
        );
    }
}

#[test]
fn pending_and_resolved_previews_paint_hint_tiles_and_text() {
    for axis in [None, Some(SplitAxis::Vertical), Some(SplitAxis::Horizontal)] {
        let overlay = overlay(axis, (100.0, 200.0), (300.0, 30.0));
        let shapes = render_once(|ui| paint_split_preview(ui, &overlay));
        assert!(shapes.len() >= 6, "{axis:?}: {}", shapes.len());
    }
}
