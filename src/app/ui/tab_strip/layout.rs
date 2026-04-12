use crate::app::app_state::ScratchpadApp;
use crate::app::theme::*;

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
