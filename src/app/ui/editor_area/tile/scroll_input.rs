use crate::app::ui::scrolling;
use eframe::egui;

pub(super) fn local_scroll_source(
    _egui_vis: egui::scroll_area::ScrollBarVisibility,
) -> scrolling::ScrollSource {
    // Wheel and scrollbar input should be handled by the local scroll
    // container so hover gating, clipping, and same-frame content offsets stay
    // in one path. Selection edge autoscroll remains editor-specific.
    scrolling::ScrollSource {
        scroll_bar: true,
        mouse_wheel: true,
        drag: false,
        programmatic: true,
    }
}

pub(super) fn scrollbar_policy_from_egui(
    vis: egui::scroll_area::ScrollBarVisibility,
) -> scrolling::ScrollbarPolicy {
    use egui::scroll_area::ScrollBarVisibility;
    match vis {
        ScrollBarVisibility::AlwaysVisible => scrolling::ScrollbarPolicy::AlwaysVisible,
        ScrollBarVisibility::AlwaysHidden => scrolling::ScrollbarPolicy::Hidden,
        ScrollBarVisibility::VisibleWhenNeeded => scrolling::ScrollbarPolicy::VisibleWhenNeeded,
    }
}

pub(super) fn resolve_editor_scroll_offset_override(
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
    layout_requested_scroll_offset: Option<egui::Vec2>,
    wheel_requested_scroll_offset: Option<egui::Vec2>,
    scrollbar_requested_scroll_offset: Option<egui::Vec2>,
) -> Option<egui::Vec2> {
    scrollbar_requested_scroll_offset
        .or(wheel_requested_scroll_offset)
        .or(layout_requested_scroll_offset)
        .map(|offset| clamp_scroll_offset(offset, content_size, viewport_size))
}

fn clamp_scroll_offset(
    offset: egui::Vec2,
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
) -> egui::Vec2 {
    let max_offset = max_scroll_offset(content_size, viewport_size);
    egui::vec2(
        offset.x.clamp(0.0, max_offset.x),
        offset.y.clamp(0.0, max_offset.y),
    )
}

fn max_scroll_offset(content_size: egui::Vec2, viewport_size: egui::Vec2) -> egui::Vec2 {
    egui::vec2(
        (content_size.x - viewport_size.x).max(0.0),
        (content_size.y - viewport_size.y).max(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_local_scroll_source_accepts_mouse_wheel() {
        let source = local_scroll_source(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded);

        assert!(source.mouse_wheel);
        assert!(source.scroll_bar);
        assert!(!source.drag);
    }

    #[test]
    fn scrollbar_offset_wins_over_wheel_and_layout_requests() {
        let result = resolve_editor_scroll_offset_override(
            egui::vec2(1000.0, 1000.0),
            egui::vec2(100.0, 100.0),
            Some(egui::vec2(10.0, 10.0)),
            Some(egui::vec2(20.0, 20.0)),
            Some(egui::vec2(30.0, 30.0)),
        );

        assert_eq!(result, Some(egui::vec2(30.0, 30.0)));
    }

    #[test]
    fn editor_scroll_override_clamps_to_content_bounds() {
        let result = resolve_editor_scroll_offset_override(
            egui::vec2(300.0, 200.0),
            egui::vec2(100.0, 80.0),
            Some(egui::vec2(500.0, 500.0)),
            None,
            None,
        );

        assert_eq!(result, Some(egui::vec2(200.0, 120.0)));
    }
}
