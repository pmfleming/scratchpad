use super::widgets::{
    EDITOR_CONTEXT_CARET_WIDTH, EDITOR_CONTEXT_MENU_WIDTH, EDITOR_CONTEXT_ROW_HEIGHT,
    EDITOR_UNICODE_DESCRIPTION_X, EDITOR_UNICODE_DIVIDER_X, EDITOR_UNICODE_INSERT_SUBMENU_WIDTH,
    EDITOR_UNICODE_LABEL_X, apply_context_menu_row_hover_style, menu_action_button,
    paint_context_menu_row_label, set_menu_width,
};
use crate::app::app_state::{
    ScratchpadApp,
    workspace::{accessors as workspace_accessors, editing as workspace_editing},
};
use crate::app::theme::{border, text_muted, text_primary};
use crate::app::ui::{visuals::with_visual_overrides, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_U_UP_LEFT, CARET_RIGHT, TEXT_AA, TEXT_ALIGN_JUSTIFY, TEXT_ALIGN_LEFT, TEXT_ALIGN_RIGHT,
};

#[derive(Clone, Copy)]
struct UnicodeControlChar {
    short_label: &'static str,
    description: &'static str,
    value: &'static str,
}

const UNICODE_CONTROL_CHARS: &[UnicodeControlChar] = &[
    UnicodeControlChar {
        short_label: "LRM",
        description: "Left-to-right mark",
        value: "\u{200E}",
    },
    UnicodeControlChar {
        short_label: "RLM",
        description: "Right-to-left mark",
        value: "\u{200F}",
    },
    UnicodeControlChar {
        short_label: "ZWJ",
        description: "Zero-width joiner",
        value: "\u{200D}",
    },
    UnicodeControlChar {
        short_label: "ZWNJ",
        description: "Zero-width non-joiner",
        value: "\u{200C}",
    },
    UnicodeControlChar {
        short_label: "LRE",
        description: "Start of left-to-right embedding",
        value: "\u{202A}",
    },
    UnicodeControlChar {
        short_label: "RLE",
        description: "Start of right-to-left embedding",
        value: "\u{202B}",
    },
    UnicodeControlChar {
        short_label: "LRO",
        description: "Start of left-to-right override",
        value: "\u{202D}",
    },
    UnicodeControlChar {
        short_label: "RLO",
        description: "Start of right-to-left override",
        value: "\u{202E}",
    },
    UnicodeControlChar {
        short_label: "PDF",
        description: "Pop directional formatting",
        value: "\u{202C}",
    },
    UnicodeControlChar {
        short_label: "NADS",
        description: "National digit shapes substitution",
        value: "\u{206E}",
    },
    UnicodeControlChar {
        short_label: "NODS",
        description: "Nominal (European) digit shapes",
        value: "\u{206F}",
    },
    UnicodeControlChar {
        short_label: "ASS",
        description: "Activate symmetric swapping",
        value: "\u{206B}",
    },
    UnicodeControlChar {
        short_label: "ISS",
        description: "Inhibit symmetric swapping",
        value: "\u{206A}",
    },
    UnicodeControlChar {
        short_label: "AAFS",
        description: "Activate Arabic form shaping",
        value: "\u{206D}",
    },
    UnicodeControlChar {
        short_label: "IAFS",
        description: "Inhibit Arabic form shaping",
        value: "\u{206C}",
    },
    UnicodeControlChar {
        short_label: "RS",
        description: "Record separator (Block separator)",
        value: "\u{001E}",
    },
    UnicodeControlChar {
        short_label: "US",
        description: "Unit separator (Segment separator)",
        value: "\u{001F}",
    },
];

pub(super) fn render_display_unicode_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    unicode_submenu_row(ui, "Unicode", TEXT_AA, |ui| {
        render_unicode_menu_contents(ui, app);
    });
}

fn render_unicode_menu_contents(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    set_menu_width(ui, EDITOR_CONTEXT_MENU_WIDTH);
    let buffer = app
        .tab_manager
        .active_tab()
        .and_then(|tab| tab.buffer_for_view(tab.layout.active_view_id));
    let right_to_left = buffer.is_some_and(|buffer| buffer.right_to_left_reading_order);
    let show_control_chars = buffer.is_some_and(|buffer| buffer.show_control_chars);
    let control_chars_available =
        buffer.is_some_and(|buffer| buffer.has_visible_control_substitutions());
    let (direction_label, direction_icon) = if right_to_left {
        ("Left to Right", TEXT_ALIGN_LEFT)
    } else {
        ("Right to Left", TEXT_ALIGN_RIGHT)
    };

    if menu_action_button(ui, direction_label, Some(direction_icon), true) {
        workspace_editing::toggle_active_buffer_reading_order(app);
        ui.close();
    }

    let control_icon = if show_control_chars {
        "¶"
    } else {
        TEXT_ALIGN_JUSTIFY
    };
    if menu_action_button(
        ui,
        "Control Chars",
        Some(control_icon),
        show_control_chars || control_chars_available,
    ) {
        workspace_editing::toggle_active_buffer_control_chars(app);
        ui.close();
    }

    unicode_submenu_row(ui, "Insert Control", TEXT_AA, |ui| {
        render_insert_control_menu(ui, app);
    });
    menu_action_button(ui, "Reconversion", Some(ARROW_U_UP_LEFT), false);
}

fn render_insert_control_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    set_menu_width(ui, EDITOR_UNICODE_INSERT_SUBMENU_WIDTH);
    for control in UNICODE_CONTROL_CHARS {
        if unicode_control_char_button(ui, control) {
            workspace_editing::insert_text_in_active_view(app, control.value);
            workspace_accessors::request_focus_for_active_view(app);
            ui.close();
        }
    }
}

fn unicode_submenu_row(
    ui: &mut egui::Ui,
    label: &str,
    icon: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let button = egui::Button::new("")
            .min_size(egui::vec2(
                EDITOR_CONTEXT_MENU_WIDTH,
                EDITOR_CONTEXT_ROW_HEIGHT,
            ))
            .stroke(egui::Stroke::NONE);

        let (response, _) =
            widget_ids::surface_widget(ui, ("editor_context.submenu", label), "submenu", |ui| {
                egui::containers::menu::SubMenuButton::from_button(button).ui(ui, |ui| {
                    add_contents(ui);
                })
            })
            .inner;

        let rect = response.rect;
        paint_context_menu_row_label(ui, rect, Some(icon), label, true);
        ui.painter().text(
            rect.right_center() - egui::vec2(EDITOR_CONTEXT_CARET_WIDTH * 0.5, 0.0),
            egui::Align2::CENTER_CENTER,
            CARET_RIGHT,
            egui_phosphor::font_id(egui::TextStyle::Button.resolve(ui.style()).size),
            text_primary(ui),
        );
    });
}

fn unicode_control_char_button(ui: &mut egui::Ui, control: &UnicodeControlChar) -> bool {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let response = widget_ids::surface_response(
            ui,
            ("editor_context.unicode_control", control.short_label),
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add(
                    egui::Button::new("")
                        .min_size(egui::vec2(
                            EDITOR_UNICODE_INSERT_SUBMENU_WIDTH,
                            EDITOR_CONTEXT_ROW_HEIGHT,
                        ))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        paint_unicode_control_char_row(ui, response.rect, control);
        response.clicked()
    })
}

fn paint_unicode_control_char_row(ui: &egui::Ui, rect: egui::Rect, control: &UnicodeControlChar) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    ui.painter().text(
        rect.left_center() + egui::vec2(EDITOR_UNICODE_LABEL_X, 0.0),
        egui::Align2::LEFT_CENTER,
        control.short_label,
        font.clone(),
        text_primary(ui),
    );
    let divider_x = rect.left() + EDITOR_UNICODE_DIVIDER_X;
    ui.painter().line_segment(
        [
            egui::pos2(divider_x, rect.top() + 5.0),
            egui::pos2(divider_x, rect.bottom() - 5.0),
        ],
        egui::Stroke::new(1.0, border(ui).gamma_multiply(0.65)),
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(EDITOR_UNICODE_DESCRIPTION_X, 0.0),
        egui::Align2::LEFT_CENTER,
        control.description,
        font,
        text_muted(ui),
    );
}
