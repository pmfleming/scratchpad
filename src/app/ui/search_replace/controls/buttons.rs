use crate::app::app_state::{SearchReplaceAvailability, SearchScope, SearchScopeOrigin};
use crate::app::services::search::SearchMode;
use crate::app::shortcut_tooltips;
use crate::app::theme::{
    action_hover_bg, border, tab_selected_accent, tab_selected_bg, text_primary,
};
use crate::app::ui::{callout, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{CARDS, RECTANGLE, TABS, TEXT_ALIGN_JUSTIFY};

const CONTROL_BUTTON_HEIGHT: f32 = 34.0;
const REGEX_ICON: &str = ".*";

pub(super) const ICON_BUTTON_SIZE: egui::Vec2 = egui::vec2(36.0, CONTROL_BUTTON_HEIGHT);

pub(super) fn icon_toggle_chip(
    ui: &mut egui::Ui,
    selected: bool,
    icon: &str,
    tooltip: &str,
) -> egui::Response {
    chip_button(
        ui,
        egui::RichText::new(icon)
            .font(egui::FontId::proportional(16.0))
            .color(if selected {
                text_primary(ui)
            } else {
                text_primary(ui).gamma_multiply(0.9)
            }),
        selected,
        ICON_BUTTON_SIZE,
        egui::vec2(0.0, 0.0),
        tooltip,
    )
}

pub(super) fn toggle_flag(ui: &mut egui::Ui, value: &mut bool, icon: &str, tooltip: &str) {
    if icon_toggle_chip(ui, *value, icon, tooltip).clicked() {
        *value = !*value;
    }
}

pub(super) fn toggle_mode(ui: &mut egui::Ui, mode: &mut SearchMode) {
    let regex_enabled = *mode == SearchMode::Regex;
    if icon_toggle_chip(
        ui,
        regex_enabled,
        REGEX_ICON,
        shortcut_tooltips::SEARCH_MODE_REGEX,
    )
    .clicked()
    {
        *mode = if regex_enabled {
            SearchMode::PlainText
        } else {
            SearchMode::Regex
        };
    }
}

pub(super) fn trigger_action(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: &str,
    tooltip: &str,
    flag: &mut bool,
) {
    if icon_action_button(ui, icon, tooltip, enabled).clicked() {
        *flag = true;
    }
}

pub(super) fn scope_tooltip(scope: SearchScope, origin: SearchScopeOrigin) -> &'static str {
    match scope {
        SearchScope::ActiveBuffer => shortcut_tooltips::SEARCH_SCOPE_CURRENT_FILE,
        SearchScope::SelectionOnly if origin == SearchScopeOrigin::SelectionDefault => {
            shortcut_tooltips::SEARCH_SCOPE_SELECTION_DEFAULT
        }
        SearchScope::SelectionOnly => shortcut_tooltips::SEARCH_SCOPE_SELECTION,
        SearchScope::ActiveWorkspaceTab => shortcut_tooltips::SEARCH_SCOPE_CURRENT_TAB,
        SearchScope::AllOpenTabs => shortcut_tooltips::SEARCH_SCOPE_ALL_TABS,
    }
}

pub(super) fn scope_icon(scope: SearchScope) -> &'static str {
    match scope {
        SearchScope::SelectionOnly => TEXT_ALIGN_JUSTIFY,
        SearchScope::ActiveBuffer => RECTANGLE,
        SearchScope::ActiveWorkspaceTab => CARDS,
        SearchScope::AllOpenTabs => TABS,
    }
}

pub(super) fn replace_tooltip<'a>(
    availability: &'a SearchReplaceAvailability,
    allowed_tooltip: &'a str,
) -> &'a str {
    match availability {
        SearchReplaceAvailability::Allowed => allowed_tooltip,
        SearchReplaceAvailability::Disabled => "Replace is unavailable until results are ready.",
        SearchReplaceAvailability::Blocked(message) => message.as_str(),
    }
}

fn icon_action_button(
    ui: &mut egui::Ui,
    icon: &str,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    callout::icon_button(
        ui,
        ("search_replace.action", tooltip),
        icon,
        callout::IconButtonStyle {
            icon_size: 16.0,
            size: ICON_BUTTON_SIZE,
            fill: action_hover_bg(ui),
        },
        tooltip,
        enabled,
    )
}

fn chip_button(
    ui: &mut egui::Ui,
    text: egui::RichText,
    selected: bool,
    min_size: egui::Vec2,
    padding: egui::Vec2,
    tooltip: &str,
) -> egui::Response {
    let previous_padding = ui.spacing().button_padding;
    ui.spacing_mut().button_padding = padding;
    let response = widget_ids::surface_response(
        ui,
        ("search_replace.chip", tooltip),
        widget_ids::WidgetRole::ToggleChip,
        |ui| {
            ui.add(
                egui::Button::new(text)
                    .min_size(min_size)
                    .fill(if selected {
                        tab_selected_bg(ui)
                    } else {
                        action_hover_bg(ui)
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        if selected {
                            tab_selected_accent(ui)
                        } else {
                            border(ui)
                        },
                    ))
                    .corner_radius(egui::CornerRadius::same(8)),
            )
        },
    )
    .on_hover_text(tooltip);
    ui.spacing_mut().button_padding = previous_padding;
    response
}
