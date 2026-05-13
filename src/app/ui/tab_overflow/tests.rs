use super::{
    overflow_drag_hover_ready, overflow_drag_hover_remaining, overflow_popup_target_height,
};
use crate::app::theme::TAB_HEIGHT;

#[test]
fn overflow_popup_height_tracks_visible_rows() {
    assert_eq!(
        overflow_popup_target_height(4, TAB_HEIGHT * 20.0),
        TAB_HEIGHT * 4.0
    );
}

#[test]
fn overflow_popup_height_caps_at_viewport_max() {
    assert_eq!(
        overflow_popup_target_height(20, TAB_HEIGHT * 4.5),
        TAB_HEIGHT * 4.5
    );
}

#[test]
fn drag_hover_open_waits_for_dwell_delay() {
    assert!(!overflow_drag_hover_ready(10.0, 10.39));
    assert!(overflow_drag_hover_ready(10.0, 10.4));
}

#[test]
fn drag_hover_repaint_uses_remaining_delay() {
    assert_eq!(
        overflow_drag_hover_remaining(10.0, 10.25),
        std::time::Duration::from_millis(150)
    );
}
