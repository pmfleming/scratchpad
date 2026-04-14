use crate::app::app_state::ScratchpadApp;
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::*;
use eframe::egui::{self, Stroke};
use std::time::Instant;

const VERTICAL_TAB_LIST_PADDING: f32 = 8.0;
pub(crate) const AUTO_HIDE_PEEK_SIZE: f32 = 6.0;
const AUTO_HIDE_REVEAL_MARGIN: f32 = 12.0;

fn auto_hide_visible(
    app: &mut ScratchpadApp,
    ctx: &egui::Context,
    has_context: bool,
    now: Instant,
) -> bool {
    if !app.auto_hide_tab_list() {
        return true;
    }

    if has_context {
        app.keep_tab_list_open();
        return true;
    }

    if let Some(deadline) = app.vertical_tab_list_hide_deadline {
        if deadline > now {
            ctx.request_repaint_after(deadline.saturating_duration_since(now));
            return true;
        }

        app.close_tab_list();
        return false;
    }

    if app.vertical_tab_list_open {
        app.delay_tab_list_hide(now);
        ctx.request_repaint_after(app.tab_list_auto_hide_delay());
        return true;
    }

    false
}

pub(crate) struct HeaderLayout {
    pub spacing: f32,
    pub caption_controls_width: f32,
    pub has_overflow: bool,
    pub visible_strip_width: f32,
    pub drag_width: f32,
    pub tab_area_width: f32,
}

impl HeaderLayout {
    pub fn measure(
        app: &ScratchpadApp,
        remaining_width: f32,
        spacing: f32,
        include_tabs: bool,
    ) -> Self {
        let caption_controls_width =
            CAPTION_BUTTON_SIZE.x * 3.0 + CAPTION_BUTTON_SPACING * 2.0 + CAPTION_TRAILING_PADDING;
        let tab_action_width = BUTTON_SIZE.x;
        let overflow_button_width = BUTTON_SIZE.x;
        let spacer_before_captions = 8.0;
        if !include_tabs {
            let tab_area_width =
                (remaining_width - caption_controls_width - spacer_before_captions).max(0.0);
            return Self {
                spacing,
                caption_controls_width,
                has_overflow: false,
                visible_strip_width: 0.0,
                drag_width: tab_area_width,
                tab_area_width,
            };
        }

        let viewport_width_with_overflow = (remaining_width
            - caption_controls_width
            - spacer_before_captions
            - tab_action_width
            - spacing
            - overflow_button_width
            - spacing)
            .max(0.0);
        let total_tab_width = app.estimated_tab_strip_width(spacing);
        let has_overflow = total_tab_width > viewport_width_with_overflow;
        let viewport_width = (remaining_width
            - caption_controls_width
            - spacer_before_captions
            - tab_action_width
            - spacing
            - if has_overflow {
                overflow_button_width + spacing
            } else {
                0.0
            })
        .max(0.0);
        let visible_strip_width = total_tab_width.min(viewport_width);
        let drag_width = (viewport_width - visible_strip_width).max(0.0);
        let tab_area_width =
            (remaining_width - caption_controls_width - spacer_before_captions).max(0.0);

        Self {
            spacing,
            caption_controls_width,
            has_overflow,
            visible_strip_width,
            drag_width,
            tab_area_width,
        }
    }
}

fn pointer_near_bar(ui: &egui::Ui, expanded_size: f32, position: TabListPosition) -> bool {
    ui.input(|input| {
        input.pointer.hover_pos().is_some_and(|pos| match position {
            TabListPosition::Top => {
                pos.y <= ui.max_rect().top() + expanded_size + AUTO_HIDE_REVEAL_MARGIN
            }
            TabListPosition::Bottom => {
                pos.y >= ui.max_rect().bottom() - expanded_size - AUTO_HIDE_REVEAL_MARGIN
            }
            TabListPosition::Left => {
                pos.x <= ui.max_rect().left() + expanded_size + AUTO_HIDE_REVEAL_MARGIN
            }
            TabListPosition::Right => {
                pos.x >= ui.max_rect().right() - expanded_size - AUTO_HIDE_REVEAL_MARGIN
            }
        })
    })
}

pub(crate) fn horizontal_bar_visible(
    ui: &egui::Ui,
    app: &mut ScratchpadApp,
    position: TabListPosition,
    now: Instant,
) -> bool {
    auto_hide_visible(
        app,
        ui.ctx(),
        pointer_near_bar(ui, HEADER_HEIGHT, position),
        now,
    )
}

pub(crate) fn auto_hide_panel_extent(visible: bool, expanded_size: f32) -> f32 {
    if visible {
        expanded_size
    } else {
        AUTO_HIDE_PEEK_SIZE
    }
}

pub(crate) fn horizontal_tab_list_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::NONE
        .fill(header_bg(ui))
        .stroke(Stroke::new(1.0, border(ui)))
        .inner_margin(egui::Margin {
            left: HEADER_LEFT_PADDING as i8,
            right: HEADER_RIGHT_PADDING as i8,
            top: HEADER_VERTICAL_PADDING as i8,
            bottom: HEADER_VERTICAL_PADDING as i8,
        })
}

pub(crate) fn show_horizontal_tab_panel(
    ui: &mut egui::Ui,
    position: TabListPosition,
    panel_id: &'static str,
    bar_visible: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let panel = match position {
        TabListPosition::Top => egui::Panel::top(panel_id),
        TabListPosition::Bottom => egui::Panel::bottom(panel_id),
        TabListPosition::Left | TabListPosition::Right => {
            unreachable!("horizontal tab panel only supports top/bottom")
        }
    };

    panel
        .exact_size(auto_hide_panel_extent(bar_visible, HEADER_HEIGHT))
        .frame(horizontal_tab_list_frame(ui))
        .show_inside(ui, |ui| {
            if !bar_visible {
                return;
            }
            add_contents(ui);
        });
}

pub(crate) fn show_horizontal_edge_tab_list(
    ui: &mut egui::Ui,
    position: TabListPosition,
    panel_id: &'static str,
    selected: bool,
    bar_visible: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    if !selected {
        return;
    }

    show_horizontal_tab_panel(ui, position, panel_id, bar_visible, add_contents);
}

pub(crate) fn vertical_tab_list_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::NONE
        .fill(header_bg(ui))
        .stroke(Stroke::new(1.0, border(ui)))
        .inner_margin(egui::Margin::same(VERTICAL_TAB_LIST_PADDING as i8))
}

pub(crate) fn vertical_panel_visible(
    ui: &egui::Ui,
    app: &mut ScratchpadApp,
    side: TabListPosition,
    now: Instant,
) -> bool {
    auto_hide_visible(
        app,
        ui.ctx(),
        pointer_near_bar(ui, app.vertical_tab_list_width(), side),
        now,
    )
}

pub(crate) fn vertical_tab_panel(side: TabListPosition, visible: bool) -> egui::Panel {
    match (side, visible) {
        (TabListPosition::Left, true) => egui::Panel::left("vertical_tab_list_left"),
        (TabListPosition::Left, false) => egui::Panel::left("vertical_tab_list_left_peek"),
        (TabListPosition::Right, true) => egui::Panel::right("vertical_tab_list_right"),
        (TabListPosition::Right, false) => egui::Panel::right("vertical_tab_list_right_peek"),
        (TabListPosition::Top, _) | (TabListPosition::Bottom, _) => {
            unreachable!("vertical tab panel only supports left/right")
        }
    }
}
