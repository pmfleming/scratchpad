use crate::app::services::settings_store::{TabListPosition, TabOrderDirection, TabOrderMode};
use crate::app::theme::{
    TAB_BUTTON_WIDTH, TAB_LIST_SCROLLBAR_GUTTER, action_bg, action_hover_bg, border, tab_active_bg,
    tab_selected_accent, text_primary,
};
use crate::app::ui::widget_ids;
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, ARROW_UP, CARET_RIGHT, SORT_ASCENDING, SORT_DESCENDING,
};
use std::hash::Hash;
use std::path::Path;

pub(super) const WIDTH: f32 = 220.0;
pub(super) const SUBMENU_WIDTH: f32 = 176.0;
pub(super) const ORDER_SUBMENU_WIDTH: f32 = 116.0;
pub(super) const OPEN_FILE_SUBMENU_WIDTH: f32 = TAB_BUTTON_WIDTH + TAB_LIST_SCROLLBAR_GUTTER;
pub(super) const ROW_HEIGHT: f32 = 28.0;
pub(super) const CARET_WIDTH: f32 = 28.0;
pub(super) const ORDER_DIRECTION_BUTTON_SIZE: egui::Vec2 = egui::vec2(38.0, 30.0);
pub(super) const OPEN_DISPOSITION_BUTTON_SIZE: egui::Vec2 = egui::vec2(38.0, 30.0);

const ICON_CENTER_X: f32 = 20.0;
const LABEL_X: f32 = 52.0;
const NO_ICON_LABEL_X: f32 = 18.0;

pub(super) fn menu_button(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    icon: Option<&str>,
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
        paint_row_label(ui, response.rect, icon, label, enabled);
        response.clicked()
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
        paint_row_label(ui, response.rect, None, label, true);
        if selected {
            paint_selected_row_outline(ui, response.rect);
        }
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
            .corner_radius(egui::CornerRadius::same(8));

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
            egui::FontId::proportional(17.0),
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
            .corner_radius(egui::CornerRadius::same(8));

        let response =
            widget_ids::surface_response(ui, id_source, widget_ids::WidgetRole::IconButton, |ui| {
                ui.add(button)
            });
        ui.painter().text(
            response.rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(17.0),
            text_primary(ui),
        );
        let clicked = response.clicked() || response.clicked_by(egui::PointerButton::Secondary);
        response.on_hover_text(tooltip);
        clicked
    })
}

pub(super) fn recent_file_button(
    ui: &mut egui::Ui,
    id_source: impl Hash,
    width: f32,
    path: &Path,
) -> bool {
    let exists = path.is_file();
    let label = recent_file_label(path);
    with_row_visuals(ui, |ui| {
        let response = widget_ids::surface_response(
            ui,
            id_source,
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add_enabled(
                    exists,
                    egui::Button::new("")
                        .min_size(egui::vec2(width, ROW_HEIGHT))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        paint_recent_file_label(ui, response.rect, &label, exists);
        let clicked = response.clicked() && exists;
        response.on_hover_text(path.display().to_string());
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

fn with_visual_overrides<R>(
    ui: &mut egui::Ui,
    configure: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let previous_visuals = ui.visuals().clone();
    configure(ui);
    let result = add_contents(ui);
    *ui.visuals_mut() = previous_visuals;
    result
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
        paint_row_label(ui, response.rect, Some(icon), label, enabled);
        response.clicked()
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
        let button = egui::Button::new(egui::RichText::new(CARET_RIGHT).color(text_primary(ui)))
            .min_size(egui::vec2(CARET_WIDTH, ROW_HEIGHT))
            .stroke(egui::Stroke::NONE);

        widget_ids::surface_widget(ui, id_source, "submenu", |ui| {
            egui::containers::menu::SubMenuButton::from_button(button)
                .config(
                    egui::containers::menu::MenuConfig::new()
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
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
        "Close Down"
    } else {
        "Close Right"
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

pub(super) fn tab_list_position_label(position: TabListPosition) -> &'static str {
    match position {
        TabListPosition::Top => "Top",
        TabListPosition::Bottom => "Bottom",
        TabListPosition::Left => "Left",
        TabListPosition::Right => "Right",
    }
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
    enabled: bool,
) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let color = if enabled {
        text_primary(ui)
    } else {
        text_primary(ui).gamma_multiply(0.45)
    };
    if let Some(icon) = icon {
        ui.painter().text(
            rect.left_center() + egui::vec2(ICON_CENTER_X, 0.0),
            egui::Align2::CENTER_CENTER,
            icon,
            font.clone(),
            color,
        );
    }
    ui.painter().text(
        rect.left_center() + egui::vec2(label_x(icon), 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
}

fn paint_recent_file_label(ui: &egui::Ui, rect: egui::Rect, label: &str, exists: bool) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let color = if exists {
        text_primary(ui)
    } else {
        missing_file_color(ui)
    };
    let text_rect = egui::Rect::from_min_max(
        rect.left_top() + egui::vec2(8.0, 0.0),
        rect.right_bottom() - egui::vec2(8.0, 0.0),
    );
    ui.painter().with_clip_rect(text_rect).text(
        text_rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        label,
        font,
        color,
    );

    if !exists {
        let y = text_rect.center().y + 1.0;
        ui.painter().line_segment(
            [
                egui::pos2(text_rect.left(), y),
                egui::pos2(text_rect.right(), y),
            ],
            egui::Stroke::new(1.0, color),
        );
    }
}

fn recent_file_label(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn missing_file_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(238, 96, 96)
    } else {
        egui::Color32::from_rgb(184, 36, 36)
    }
}

fn label_x(icon: Option<&str>) -> f32 {
    if icon.is_some() {
        LABEL_X
    } else {
        NO_ICON_LABEL_X
    }
}

fn paint_selected_row_outline(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().rect_stroke(
        rect.shrink2(egui::vec2(5.0, 3.0)),
        4.0,
        egui::Stroke::new(1.5, tab_selected_accent(ui).gamma_multiply(0.95)),
        egui::StrokeKind::Outside,
    );
}

fn apply_row_hover_style(ui: &mut egui::Ui) {
    let hover_bg = action_hover_bg(ui);
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    for widgets in [
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widgets.bg_fill = hover_bg;
        widgets.weak_bg_fill = hover_bg;
        widgets.bg_stroke = egui::Stroke::NONE;
    }
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
}

fn with_row_visuals<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let previous_visuals = ui.visuals().clone();
    apply_row_hover_style(ui);
    let result = add_contents(ui);
    *ui.visuals_mut() = previous_visuals;
    result
}
