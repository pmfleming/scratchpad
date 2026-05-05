use crate::app::ui::{callout, scrolling};
use eframe::egui;

pub(super) fn local_scroll_source(
    _egui_vis: egui::scroll_area::ScrollBarVisibility,
) -> scrolling::ScrollSource {
    // Editor handles its own pointer wheel + drag (selection edges, cursor
    // reveal suppression). Scrollbar drag and programmatic targets go through
    // the local container.
    scrolling::ScrollSource {
        scroll_bar: true,
        mouse_wheel: false,
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

pub(super) fn requested_scroll_offset_for_pointer_wheel(
    ui: &egui::Ui,
    current_offset: egui::Vec2,
) -> Option<egui::Vec2> {
    if callout::scroll_blocker_active(ui.ctx()) {
        return None;
    }
    if !pointer_over_rect(ui, ui.max_rect()) {
        return None;
    }

    scroll_offset_from_wheel_delta(current_offset, ui.input(|input| input.smooth_scroll_delta))
}

fn pointer_over_rect(ui: &egui::Ui, rect: egui::Rect) -> bool {
    ui.input(|input| {
        input
            .pointer
            .hover_pos()
            .is_some_and(|pos| rect.contains(pos))
    })
}

fn scroll_offset_from_wheel_delta(
    current_offset: egui::Vec2,
    scroll_delta: egui::Vec2,
) -> Option<egui::Vec2> {
    let desired = egui::vec2(
        (current_offset.x - scroll_delta.x).max(0.0),
        (current_offset.y - scroll_delta.y).max(0.0),
    );
    (desired != current_offset).then_some(desired)
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
