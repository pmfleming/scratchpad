use super::{
    ScratchpadApp, SettingsUi, action_bg, action_hover_bg, border, category_heading, egui,
    phosphor_button, text_muted, text_primary,
};
use crate::app::ui::widget_ids;
mod card;
mod layout_state;
mod primitives;
pub(super) use card::{card_header, render_preview_panel, settings_card_frame};
use card::{card_header_with_trailing_width, clickable_card_header, label_stack};
#[cfg(test)]
pub(super) use layout_state::{
    SettingsControlMeasurement, reset_settings_layout_measurements, settings_card_measurements,
    settings_control_measurements,
};
use layout_state::{active_settings_control_lane, with_settings_control_lane};
use primitives::{path_tail_text, toggle_switch, value_pill};
use std::hash::Hash;
use std::ops::RangeInclusive;

pub(super) fn record_settings_control_box(label: impl Into<String>, rect: egui::Rect) {
    layout_state::record_settings_control_box(label, rect);
}

pub(super) struct CategoryCard<'a> {
    pub heading: &'a str,
    pub id_source: &'a str,
    pub icon: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub default_open: bool,
}

pub(super) struct ComboSelectRow<'a, T, SelectedLabel, OptionLabel, OnChange> {
    pub label: &'a str,
    pub description: Option<&'a str>,
    pub combo_id: &'a str,
    pub record_label: &'a str,
    pub current: T,
    pub options: &'a [T],
    pub selected_label: SelectedLabel,
    pub option_label: OptionLabel,
    pub on_change: OnChange,
}

pub(super) fn expandable_card(
    ui: &mut egui::Ui,
    id_source: &str,
    icon: &str,
    title: &str,
    description: &str,
    default_open: bool,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    #[cfg(test)]
    let _ = default_open;
    #[cfg(test)]
    let default_open = true;

    let id = widget_ids::local(ui, id_source);
    let is_open = ui
        .data_mut(|data| data.get_persisted::<bool>(id))
        .unwrap_or(default_open);

    settings_card_frame(ui, |ui| {
        let response = clickable_card_header(ui, id, icon, title, Some(description), |ui| {
            let chevron = if is_open {
                egui_phosphor::regular::CARET_UP
            } else {
                egui_phosphor::regular::CARET_DOWN
            };
            ui.label(
                egui::RichText::new(chevron)
                    .size(18.0)
                    .color(SettingsUi::icon_color(ui)),
            );
        });

        if response.clicked() {
            ui.data_mut(|data| data.insert_persisted(id, !is_open));
        }

        if is_open {
            inner_divider(ui);
            ui.add_space(4.0);
            add_body(ui);
        }
    });
}

pub(super) fn toggle_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: &str,
    current_value: bool,
    on_change: impl FnOnce(bool),
) {
    let mut next_value = current_value;
    settings_card_frame(ui, |ui| {
        card_header(ui, icon, title, Some(description), |ui| {
            toggle_control(ui, ("settings.toggle_card", title), &mut next_value);
        });
    });

    if next_value != current_value {
        on_change(next_value);
    }
}

pub(super) fn category_card(
    ui: &mut egui::Ui,
    card: CategoryCard<'_>,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    category_heading(ui, card.heading);
    expandable_card(
        ui,
        card.id_source,
        card.icon,
        card.title,
        card.description,
        card.default_open,
        add_body,
    );
}

pub(super) fn toggle_control(ui: &mut egui::Ui, surface_key: impl Hash, value: &mut bool) {
    available_width_control(ui, |ui| {
        let label_text = if *value { "On" } else { "Off" };
        let label_width = ui
            .painter()
            .layout_no_wrap(
                label_text.to_owned(),
                egui::FontId::proportional(SettingsUi::TYPOGRAPHY.body),
                text_primary(ui),
            )
            .rect
            .width();
        let switch_size = egui::vec2(42.0, 22.0);
        let group_width = label_width + SettingsUi::CONTROLS.gap + switch_size.x;
        let available = ui.available_rect_before_wrap();
        let group_rect = egui::Rect::from_min_size(
            egui::pos2(available.right() - group_width, available.top()),
            egui::vec2(group_width, ui.available_height()),
        );

        widget_ids::rect_scope_with_layout(
            ui,
            group_rect,
            ("settings.toggle_control.group", label_text),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(group_width);
                ui.set_max_width(group_width);
                let label_response =
                    ui.label(egui::RichText::new(label_text).color(text_primary(ui)));
                ui.add_space(SettingsUi::CONTROLS.gap);
                let response = toggle_switch(ui, surface_key, value);
                if response.changed() {
                    ui.ctx().request_repaint();
                }
                record_settings_control_box("toggle_control.label", label_response.rect);
                record_settings_control_box("toggle_control.switch", response.rect);
                record_settings_control_box(
                    "toggle_control",
                    response.rect.union(label_response.rect),
                );
            },
        );
    });
}

pub(super) fn combo_control<T>(
    ui: &mut egui::Ui,
    id_source: impl Hash,
    record_label: impl Into<String>,
    selected: &mut T,
    options: &[T],
    selected_label: impl Fn(T) -> &'static str,
    option_label: impl Fn(T) -> &'static str,
) where
    T: Copy + PartialEq,
{
    available_width_control(ui, |ui| {
        let dropdown_width = SettingsUi::dropdown_width(ui);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = widget_ids::combo_box(ui, id_source)
                .selected_text(selected_label(*selected))
                .truncate()
                .width(dropdown_width)
                .show_ui(ui, |ui| {
                    for &option in options {
                        ui.selectable_value(selected, option, option_label(option));
                    }
                })
                .response;
            record_settings_control_box(record_label, response.rect);
        });
    });
}

pub(super) fn combo_select_row<T, SelectedLabel, OptionLabel, OnChange>(
    ui: &mut egui::Ui,
    row: ComboSelectRow<'_, T, SelectedLabel, OptionLabel, OnChange>,
) where
    T: Copy + PartialEq,
    SelectedLabel: Fn(T) -> &'static str,
    OptionLabel: Fn(T) -> &'static str,
    OnChange: FnOnce(T),
{
    let mut selected = row.current;
    inner_select_row(ui, row.label, row.description, |ui| {
        combo_control(
            ui,
            row.combo_id,
            row.record_label,
            &mut selected,
            row.options,
            row.selected_label,
            row.option_label,
        );
    });
    if selected != row.current {
        (row.on_change)(selected);
    }
}

pub(super) fn toggle_select_row(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    surface_key: impl Hash,
    current: bool,
    on_change: impl FnOnce(bool),
) {
    let mut selected = current;
    inner_select_row(ui, label, description, |ui| {
        toggle_control(ui, surface_key, &mut selected);
    });
    if selected != current {
        on_change(selected);
    }
}

pub(super) fn slider_value_control(
    ui: &mut egui::Ui,
    record_label: impl Into<String>,
    value_width: f32,
    value_text: impl Into<egui::WidgetText>,
    add_slider: impl FnOnce(&mut egui::Ui, f32) -> egui::Response,
) {
    let lane_bounds = active_settings_control_lane().unwrap_or_else(|| ui.max_rect());
    let lane_width = lane_bounds
        .width()
        .clamp(0.0, SettingsUi::CONTROLS.column_width);
    let lane_rect = egui::Rect::from_min_size(
        egui::pos2(lane_bounds.right() - lane_width, lane_bounds.top()),
        egui::vec2(lane_width, SettingsUi::LAYOUT.inner_row_height),
    );
    let slider_width = lane_width
        .clamp(0.0, SettingsUi::LAYOUT.card_max_width / 3.0)
        .min((lane_width - SettingsUi::CONTROLS.gap - value_width).max(0.0));
    let group_width = (value_width + SettingsUi::CONTROLS.gap + slider_width).min(lane_width);
    let group_rect = egui::Rect::from_min_size(
        egui::pos2(lane_rect.right() - group_width, lane_rect.top()),
        egui::vec2(group_width, SettingsUi::LAYOUT.inner_row_height),
    );
    ui.advance_cursor_after_rect(lane_rect);

    widget_ids::rect_scope_with_layout(
        ui,
        group_rect,
        ("settings.slider_value_control.group", group_width.to_bits()),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(group_width);
            ui.set_max_width(group_width);
            ui.spacing_mut().slider_width = slider_width;
            let value_response =
                ui.add_sized(egui::vec2(value_width, 0.0), egui::Label::new(value_text));
            ui.add_space(SettingsUi::CONTROLS.gap);
            let slider_response = add_slider(ui, slider_width);
            record_settings_control_box(
                record_label,
                slider_response.rect.union(value_response.rect),
            );
        },
    );
}

pub(super) fn u32_slider_value_control(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    record_label: impl Into<String>,
    value: &mut u32,
    range: RangeInclusive<u32>,
    value_width: f32,
    value_text: impl Into<egui::WidgetText>,
) {
    slider_value_control(
        ui,
        record_label,
        value_width,
        value_text,
        |ui, slider_width| {
            widget_ids::surface_response(
                ui,
                surface_key,
                widget_ids::WidgetRole::ActionButton,
                |ui| {
                    ui.add_sized(
                        egui::vec2(slider_width, 0.0),
                        egui::Slider::new(value, range)
                            .step_by(1.0)
                            .show_value(false),
                    )
                },
            )
        },
    );
}

pub(super) fn nearest_option_index<T>(
    target: f32,
    options: &[T],
    option_value: impl Fn(T) -> f32,
) -> usize
where
    T: Copy,
{
    options
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            let left_distance = (target - option_value(**left)).abs();
            let right_distance = (target - option_value(**right)).abs();
            left_distance.total_cmp(&right_distance)
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

pub(super) fn settings_file_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: &str,
    app: &mut ScratchpadApp,
) {
    let mut clicked = false;
    let settings_path = app.settings_path();
    let settings_path_text = settings_path.display().to_string();
    let settings_path_tail = path_tail_text(settings_path, 3);

    settings_card_frame(ui, |ui| {
        let group_width = SettingsUi::header_trailing_width(ui).min(ui.available_width());
        card_header_with_trailing_width(ui, icon, title, Some(description), group_width, |ui| {
            let path_pill_width =
                (group_width - SettingsUi::CONTROLS.gap - SettingsUi::CONTROLS.icon_button_size)
                    .max(0.0);
            let group_rect = widget_ids::allocate_exact_rect(
                ui,
                egui::vec2(group_width, SettingsUi::LAYOUT.card_min_height),
            );
            record_settings_control_box("settings_file_card.trailing_group", group_rect);
            widget_ids::rect_scope_with_layout(
                ui,
                group_rect,
                ("settings_file_card.trailing_group", title),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.set_width(group_width);
                    ui.set_max_width(group_width);
                    clicked = phosphor_button(
                        ui,
                        ("settings_file_card", title),
                        egui_phosphor::regular::FOLDER_OPEN,
                        egui::vec2(
                            SettingsUi::CONTROLS.icon_button_size,
                            SettingsUi::CONTROLS.icon_button_size,
                        ),
                        action_bg(ui),
                        action_hover_bg(ui),
                        "Open settings file",
                    )
                    .clicked();
                    ui.add_space(SettingsUi::CONTROLS.gap);
                    let path_response = value_pill(
                        ui,
                        &settings_path_tail,
                        Some(&settings_path_text),
                        path_pill_width,
                    );
                    record_settings_control_box("settings_file_card.path_pill", path_response.rect);
                },
            );
        });
    });

    if clicked {
        app.open_settings_file_tab();
    }
}

pub(super) fn action_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: &str,
    action_tooltip: &str,
    on_click: impl FnOnce(&mut ScratchpadApp),
    app: &mut ScratchpadApp,
) {
    let mut clicked = false;
    settings_card_frame(ui, |ui| {
        card_header(ui, icon, title, Some(description), |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let response = phosphor_button(
                    ui,
                    ("settings_action_card", title),
                    icon,
                    egui::vec2(
                        SettingsUi::CONTROLS.icon_button_size,
                        SettingsUi::CONTROLS.icon_button_size,
                    ),
                    action_bg(ui),
                    action_hover_bg(ui),
                    action_tooltip,
                );
                record_settings_control_box(format!("action_card.{title}"), response.rect);
                clicked = response.clicked();
            });
        });
    });

    if clicked {
        on_click(app);
    }
}

pub(super) fn inner_select_row(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.add_space(40.0);
        let row_width = ui.available_width().max(0.0);
        let label_width = SettingsUi::row_label_width(row_width);
        let control_width = SettingsUi::row_control_width(row_width);
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, SettingsUi::LAYOUT.inner_row_height),
            egui::Layout::top_down(egui::Align::LEFT).with_main_align(egui::Align::Center),
            |ui| {
                ui.set_width(label_width);
                ui.set_max_width(label_width);
                label_stack(
                    ui,
                    label_width,
                    egui::RichText::new(label).color(text_primary(ui)),
                    description,
                );
            },
        );
        ui.add_space(SettingsUi::CONTROLS.gap);
        let control_rect = widget_ids::allocate_exact_rect(
            ui,
            egui::vec2(control_width, SettingsUi::LAYOUT.inner_row_height),
        );
        record_settings_control_box(format!("inner_select_row.{label}"), control_rect);
        widget_ids::rect_scope_with_layout(
            ui,
            control_rect,
            ("settings.inner_select_row.control", label),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(control_width);
                ui.set_max_width(control_width);
                with_settings_control_lane(control_rect, || add_control(ui));
            },
        );
    });
}

pub(super) fn available_width_control(ui: &mut egui::Ui, add_control: impl FnOnce(&mut egui::Ui)) {
    let width = active_settings_control_lane()
        .map(|lane| lane.width().clamp(0.0, SettingsUi::CONTROLS.column_width))
        .unwrap_or_else(|| SettingsUi::control_width(ui));
    let height = ui
        .available_height()
        .max(ui.spacing().interact_size.y)
        .min(SettingsUi::LAYOUT.inner_row_height);
    let rect = active_settings_control_lane()
        .map(|lane| {
            let rect = egui::Rect::from_min_size(
                egui::pos2(lane.right() - width, lane.top()),
                egui::vec2(width, height),
            );
            ui.advance_cursor_after_rect(rect);
            rect
        })
        .unwrap_or_else(|| widget_ids::allocate_exact_rect(ui, egui::vec2(width, height)));
    record_settings_control_box("available_width_control", rect);
    widget_ids::rect_scope_with_layout(
        ui,
        rect,
        ("settings.available_width_control", width.to_bits()),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(width);
            ui.set_max_width(width);
            add_control(ui);
        },
    );
}

pub(super) fn inner_divider(ui: &mut egui::Ui) {
    let width = SettingsUi::divider_width(ui);
    ui.horizontal(|ui| {
        ui.add_space(40.0);
        let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(width, 1.0));
        ui.painter()
            .rect_filled(rect, 0.0, SettingsUi::card_border(ui).gamma_multiply(0.7));
    });
}

pub(super) fn radio_option_row(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    ui.add_space(2.0);
    let width = SettingsUi::control_width(ui);
    let height = ui.spacing().interact_size.y.max(24.0);
    let mut response = widget_ids::allocate_exact_rect_interact(
        ui,
        egui::vec2(width, height),
        (
            "settings.radio_option",
            label,
            widget_ids::WidgetRole::RadioOption,
        ),
        egui::Sense::click(),
        "settings_radio_option",
    );
    if response.clicked() && !*value {
        *value = true;
        response.mark_changed();
    }

    let center_y = response.rect.center().y;
    let radio_center = egui::pos2(response.rect.left() + 9.0, center_y);
    let ring_color = if *value || response.hovered() {
        SettingsUi::accent()
    } else {
        SettingsUi::card_border(ui)
    };
    ui.painter()
        .circle_stroke(radio_center, 6.0, egui::Stroke::new(1.4, ring_color));
    if *value {
        ui.painter()
            .circle_filled(radio_center, 3.3, SettingsUi::accent());
    }

    let text_rect = egui::Rect::from_min_max(
        egui::pos2(response.rect.left() + 24.0, response.rect.top()),
        response.rect.right_top() + egui::vec2(0.0, response.rect.height()),
    );
    ui.painter().with_clip_rect(text_rect).text(
        text_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(SettingsUi::TYPOGRAPHY.body),
        text_primary(ui),
    );

    record_settings_control_box(format!("radio.{label}"), response.rect);
    response
}
