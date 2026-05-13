use super::{
    Axis, ContentExtent, ScrollAlign, ScrollIntent, ScrollManager, ViewportMetrics,
    naive_anchor_to_row, naive_row_to_anchor,
};
use eframe::egui;

fn manager() -> ScrollManager {
    let mut manager = ScrollManager::new();
    manager.set_metrics(ViewportMetrics {
        viewport_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0)),
        row_height: 10.0,
        column_width: 5.0,
        visible_rows: 10,
        visible_columns: 20,
    });
    manager.set_extent(ContentExtent {
        display_rows: 100,
        height: 1000.0,
        max_line_width: 500.0,
    });
    manager
}

fn reveal_intent(y: f32, align_y: ScrollAlign) -> ScrollIntent {
    ScrollIntent::Reveal {
        rect: egui::Rect::from_min_size(egui::pos2(0.0, y), egui::vec2(1.0, 10.0)),
        align_y: Some(align_y),
        align_x: None,
    }
}

fn apply_intent(manager: &mut ScrollManager, intent: ScrollIntent) {
    manager.apply_intent(intent, naive_anchor_to_row, naive_row_to_anchor);
}

fn scroll_y(manager: &mut ScrollManager, offset_pixels: f32) {
    apply_intent(
        manager,
        ScrollIntent::ScrollbarTo {
            axis: Axis::Y,
            offset_pixels,
        },
    );
}

fn assert_offset_y(manager: &ScrollManager, expected: f32) {
    assert_eq!(manager.pixel_offset_y(naive_anchor_to_row), expected);
}

#[test]
fn reveal_nearest_keeps_visible_rect_stationary() {
    let mut manager = manager();
    scroll_y(&mut manager, 100.0);

    apply_intent(
        &mut manager,
        reveal_intent(120.0, ScrollAlign::NearestWithMargin(5.0)),
    );

    assert_offset_y(&manager, 100.0);
}

#[test]
fn reveal_nearest_scrolls_to_hidden_rect_with_margin() {
    let mut manager = manager();

    apply_intent(
        &mut manager,
        reveal_intent(160.0, ScrollAlign::NearestWithMargin(5.0)),
    );

    assert_offset_y(&manager, 75.0);
}

#[test]
fn scrollbar_scroll_marks_user_scrolled_but_reveal_clears_it() {
    let mut manager = manager();
    scroll_y(&mut manager, 100.0);
    assert!(manager.user_scrolled());

    apply_intent(&mut manager, reveal_intent(10.0, ScrollAlign::Center));

    assert!(!manager.user_scrolled());
}
