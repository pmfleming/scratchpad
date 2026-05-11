use super::*;
use std::hash::Hash;
use std::ops::RangeInclusive;

pub(in crate::app::ui::settings) fn toggle_control(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    value: &mut bool,
) {
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

pub(in crate::app::ui::settings) fn combo_control<T>(
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

pub(in crate::app::ui::settings) fn combo_select_row<T>(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    combo_id: impl Hash,
    record_label: impl Into<String>,
    current: T,
    options: &[T],
    selected_label: impl Fn(T) -> &'static str,
    option_label: impl Fn(T) -> &'static str,
    on_change: impl FnOnce(T),
) where
    T: Copy + PartialEq,
{
    let mut selected = current;
    inner_select_row(ui, label, description, |ui| {
        combo_control(
            ui,
            combo_id,
            record_label,
            &mut selected,
            options,
            selected_label,
            option_label,
        );
    });
    if selected != current {
        on_change(selected);
    }
}

pub(in crate::app::ui::settings) fn toggle_select_row(
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

pub(in crate::app::ui::settings) fn slider_value_control(
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

pub(in crate::app::ui::settings) fn u32_slider_value_control(
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

pub(in crate::app::ui::settings) fn nearest_option_index<T>(
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

fn toggle_switch(ui: &mut egui::Ui, surface_key: impl Hash, value: &mut bool) -> egui::Response {
    let desired_size = egui::vec2(42.0, 22.0);
    let available_rect = ui.available_rect_before_wrap();
    let top = available_rect.center().y - desired_size.y * 0.5;
    let min = if ui.layout().main_dir() == egui::Direction::RightToLeft {
        egui::pos2(available_rect.right() - desired_size.x, top)
    } else {
        egui::pos2(available_rect.left(), top)
    };
    let rect = egui::Rect::from_min_size(min, desired_size);
    let mut response = widget_ids::interact(
        ui,
        rect,
        widget_ids::surface_role(surface_key, widget_ids::WidgetRole::ToggleSwitch),
        egui::Sense::click(),
        "settings_toggle_switch",
    );
    ui.advance_cursor_after_rect(rect);
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }

    let how_on = ui.ctx().animate_bool(response.id, *value);
    let radius = rect.height() * 0.5;
    let track_fill = if *value {
        SettingsUi::accent()
    } else {
        SettingsUi::control_bg(ui)
    };

    ui.painter().rect(
        rect,
        radius,
        track_fill,
        egui::Stroke::new(1.0, SettingsUi::card_border(ui)),
        egui::StrokeKind::Inside,
    );

    let thumb_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
    ui.painter().circle_filled(
        egui::pos2(thumb_x, rect.center().y),
        radius - 3.0,
        egui::Color32::WHITE,
    );

    response
}
