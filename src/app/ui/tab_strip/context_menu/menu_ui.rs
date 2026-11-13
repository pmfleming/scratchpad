use crate::app::services::settings_store::{TabListPosition, TabOrderDirection, TabOrderMode};
use crate::app::shortcut_keymap::ShortcutAction;
use crate::app::shortcut_tooltips;
use crate::app::theme::{
    action_bg, action_hover_bg, border, tab_active_bg, tab_selected_accent, tab_selected_bg,
    text_primary,
};
use crate::app::ui::{visuals::with_visual_overrides, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, ARROW_UP, CARET_RIGHT, SORT_ASCENDING, SORT_DESCENDING,
};
use std::hash::Hash;

pub(super) const WIDTH: f32 = 240.0;
pub(super) const SUBMENU_WIDTH: f32 = 176.0;
pub(super) const ORDER_SUBMENU_WIDTH: f32 = 116.0;
pub(super) const OPEN_FILE_SUBMENU_WIDTH: f32 = 380.0;
pub(super) const ROW_HEIGHT: f32 = 28.0;
pub(super) const CARET_WIDTH: f32 = 28.0;
pub(super) const ORDER_DIRECTION_BUTTON_SIZE: egui::Vec2 = egui::vec2(38.0, 30.0);
pub(super) const OPEN_DISPOSITION_BUTTON_SIZE: egui::Vec2 = egui::vec2(38.0, 30.0);

pub(super) fn menu_button(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    icon: Option<&str>,
    enabled: bool,
) -> bool {
    let keys = menu_shortcut(label).and_then(|action| shortcut_tooltips::keys(ui.ctx(), action));
    menu_value_button(ui, width, label, icon, keys.as_deref(), enabled)
}

/// A menu row that shows `value`, such as the current encoding, where a shortcut would go.
pub(super) fn menu_value_button(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    icon: Option<&str>,
    value: Option<&str>,
    enabled: bool,
) -> bool {
    with_row_visuals(ui, |ui| {
        let response = widget_ids::surface_response(
            ui,
            ("tab_context.menu_button", label),
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add_enabled(
                    enabled,
                    egui::Button::new("")
                        .min_size(egui::vec2(width, ROW_HEIGHT))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
        paint_row_label(ui, response.rect, icon, label, value, enabled);
        let clicked = response.clicked();
        if let Some(tooltip) = shortcut_tooltip_for_menu_label(ui, label) {
            response.on_hover_text(tooltip);
        }
        clicked
    })
}

pub(super) fn selectable_menu_button(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    selected: bool,
) -> bool {
    with_row_visuals(ui, |ui| {
        let response = widget_ids::surface_response(
            ui,
            ("tab_context.selectable_menu_button", label),
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add(
                    egui::Button::new("")
                        .min_size(egui::vec2(width, ROW_HEIGHT))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        if selected {
            paint_selected_row(ui, response.rect);
        }
        paint_row_label(ui, response.rect, None, label, None, true);
        response.clicked()
    })
}

pub(super) fn order_direction_button(
    ui: &mut egui::Ui,
    direction: TabOrderDirection,
    selected: bool,
) -> bool {
    let (icon, tooltip) = tab_order_direction_icon_and_tooltip(direction);
    with_order_direction_button_visuals(ui, selected, |ui| {
        let selected_stroke = if selected {
            egui::Stroke::new(1.5, tab_selected_accent(ui).gamma_multiply(0.95))
        } else {
            egui::Stroke::new(1.0, border(ui))
        };
        let button = egui::Button::new("")
            .min_size(ORDER_DIRECTION_BUTTON_SIZE)
            .stroke(selected_stroke)
            .corner_radius(egui::CornerRadius::ZERO);

        let response = widget_ids::surface_response(
            ui,
            ("tab_context.order_direction", direction),
            widget_ids::WidgetRole::IconButton,
            |ui| ui.add(button),
        );
        ui.painter().text(
            response.rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui_phosphor::font_id(17.0),
            text_primary(ui),
        );
        let clicked = response.clicked() || response.clicked_by(egui::PointerButton::Secondary);
        response.on_hover_text(tooltip);
        clicked
    })
}

pub(super) fn open_disposition_button(
    ui: &mut egui::Ui,
    id_source: impl Hash,
    icon: &str,
    tooltip: &str,
    selected: bool,
) -> bool {
    with_icon_choice_button_visuals(ui, selected, |ui| {
        let selected_stroke = if selected {
            egui::Stroke::new(1.5, tab_selected_accent(ui).gamma_multiply(0.95))
        } else {
            egui::Stroke::new(1.0, border(ui))
        };
        let button = egui::Button::new("")
            .min_size(OPEN_DISPOSITION_BUTTON_SIZE)
            .stroke(selected_stroke)
            .corner_radius(egui::CornerRadius::ZERO);

        let response =
            widget_ids::surface_response(ui, id_source, widget_ids::WidgetRole::IconButton, |ui| {
                ui.add(button)
            });
        ui.painter().text(
            response.rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui_phosphor::font_id(17.0),
            text_primary(ui),
        );
        let clicked = response.clicked() || response.clicked_by(egui::PointerButton::Secondary);
        response.on_hover_text(tooltip);
        clicked
    })
}

fn with_order_direction_button_visuals<R>(
    ui: &mut egui::Ui,
    selected: bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    with_icon_choice_button_visuals(ui, selected, add_contents)
}

fn with_icon_choice_button_visuals<R>(
    ui: &mut egui::Ui,
    selected: bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    with_visual_overrides(
        ui,
        |ui| {
            let idle_bg = if selected {
                tab_active_bg(ui)
            } else {
                action_bg(ui)
            };
            let hover_bg = action_hover_bg(ui);
            let visuals = ui.visuals_mut();
            visuals.widgets.inactive.bg_fill = idle_bg;
            visuals.widgets.inactive.weak_bg_fill = idle_bg;
            visuals.widgets.hovered.bg_fill = hover_bg;
            visuals.widgets.hovered.weak_bg_fill = hover_bg;
            visuals.widgets.active.bg_fill = hover_bg;
            visuals.widgets.active.weak_bg_fill = hover_bg;
            visuals.widgets.open.bg_fill = hover_bg;
            visuals.widgets.open.weak_bg_fill = hover_bg;
        },
        add_contents,
    )
}

pub(super) fn primary_menu_button(
    ui: &mut egui::Ui,
    id_source: impl Hash,
    label: &str,
    icon: &str,
) -> bool {
    primary_menu_button_enabled(ui, id_source, label, icon, true)
}

pub(super) fn primary_menu_button_enabled(
    ui: &mut egui::Ui,
    id_source: impl Hash,
    label: &str,
    icon: &str,
    enabled: bool,
) -> bool {
    with_row_visuals(ui, |ui| {
        let response = widget_ids::surface_response(
            ui,
            id_source,
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add_enabled(
                    enabled,
                    egui::Button::new("")
                        .min_size(egui::vec2(WIDTH - CARET_WIDTH, ROW_HEIGHT))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
        let keys =
            menu_shortcut(label).and_then(|action| shortcut_tooltips::keys(ui.ctx(), action));
        paint_row_label(
            ui,
            response.rect,
            Some(icon),
            label,
            keys.as_deref(),
            enabled,
        );
        let clicked = response.clicked();
        if let Some(tooltip) = shortcut_tooltip_for_menu_label(ui, label) {
            response.on_hover_text(tooltip);
        }
        clicked
    })
}

pub(super) fn submenu_button(
    ui: &mut egui::Ui,
    id_source: impl Hash,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    submenu_button_sized(ui, id_source, SUBMENU_WIDTH, add_contents);
}

pub(super) fn submenu_button_sized(
    ui: &mut egui::Ui,
    id_source: impl Hash,
    width: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    with_row_visuals(ui, |ui| {
        let button = egui::Button::new(
            egui::RichText::new(CARET_RIGHT)
                .font(egui_phosphor::font_id(14.0))
                .color(text_primary(ui)),
        )
        .min_size(egui::vec2(CARET_WIDTH, ROW_HEIGHT))
        .stroke(egui::Stroke::NONE);

        widget_ids::surface_widget(ui, id_source, "submenu", |ui| {
            egui::containers::menu::SubMenuButton::from_button(button)
                .config(
                    egui::containers::menu::MenuConfig::new()
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .style(crate::app::ui::panel_style::menu_style(
                            ui.visuals().window_fill,
                        )),
                )
                .ui(ui, |ui| {
                    ui.set_min_width(width);
                    ui.set_max_width(width);
                    add_contents(ui);
                });
        });
    });
}

pub(super) fn close_direction_label(position: TabListPosition) -> &'static str {
    if position.is_vertical() {
        "Close down"
    } else {
        "Close right"
    }
}

pub(super) fn close_direction_icon(position: TabListPosition) -> &'static str {
    if position.is_vertical() {
        ARROW_DOWN
    } else {
        ARROW_RIGHT
    }
}

pub(super) fn tab_order_mode_label(mode: TabOrderMode) -> &'static str {
    match mode {
        TabOrderMode::Custom => "Custom",
        TabOrderMode::FileName => "Name",
        TabOrderMode::FileSize => "Size",
        TabOrderMode::FileAge => "Saved",
        TabOrderMode::RecentEdit => "Editted",
    }
}

fn tab_order_direction_icon_and_tooltip(
    direction: TabOrderDirection,
) -> (&'static str, &'static str) {
    match direction {
        TabOrderDirection::Ascending => (SORT_ASCENDING, "Descending"),
        TabOrderDirection::Descending => (SORT_DESCENDING, "Ascending"),
    }
}

fn shortcut_tooltip_for_menu_label(ui: &egui::Ui, label: &str) -> Option<String> {
    Some(shortcut_tooltips::action(
        ui.ctx(),
        menu_shortcut(label)?,
        label,
    ))
}

fn menu_shortcut(label: &str) -> Option<ShortcutAction> {
    Some(match label {
        "New tab" => ShortcutAction::NewTab,
        "Open file" => ShortcutAction::OpenFile,
        "Open file here" => ShortcutAction::OpenFileHere,
        "Rename" => ShortcutAction::RenameTab,
        "Save" => ShortcutAction::SaveFile,
        "Encoding" => ShortcutAction::OpenEncodingDialog,
        "Copy path" => ShortcutAction::CopyActivePath,
        "Reveal in Explorer" | "Open folder" => ShortcutAction::RevealActivePath,
        "Close" => ShortcutAction::CloseTab,
        _ => return None,
    })
}

pub(super) fn tab_list_position_icon(position: TabListPosition) -> &'static str {
    match position {
        TabListPosition::Top => ARROW_UP,
        TabListPosition::Bottom => ARROW_DOWN,
        TabListPosition::Left => ARROW_LEFT,
        TabListPosition::Right => ARROW_RIGHT,
    }
}

fn paint_row_label(
    ui: &egui::Ui,
    rect: egui::Rect,
    icon: Option<&str>,
    label: &str,
    trailing: Option<&str>,
    enabled: bool,
) {
    crate::app::ui::panel_style::paint_menu_row(ui, rect, icon, label, trailing, enabled);
}

fn paint_selected_row(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().rect_filled(rect, 0, tab_selected_bg(ui));
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())),
        0,
        tab_selected_accent(ui),
    );
}

fn apply_row_hover_style(ui: &mut egui::Ui) {
    crate::app::ui::panel_style::flat_rows(ui);
}

pub(super) fn with_row_visuals<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    with_visual_overrides(ui, apply_row_hover_style, add_contents)
}
