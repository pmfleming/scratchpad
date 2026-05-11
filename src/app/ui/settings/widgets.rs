use super::*;
use crate::app::ui::widget_ids;

mod cards;
mod controls;
mod preview;

pub(in crate::app::ui::settings) use cards::*;
pub(in crate::app::ui::settings) use controls::*;
pub(in crate::app::ui::settings) use preview::*;

thread_local! {
    static SETTINGS_CONTROL_LANE: std::cell::Cell<Option<egui::Rect>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(super) struct SettingsControlMeasurement {
    pub label: String,
    pub width: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub right_x: f32,
}

#[cfg(test)]
thread_local! {
    static SETTINGS_CARD_MEASUREMENTS: std::cell::RefCell<Vec<SettingsControlMeasurement>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static SETTINGS_CONTROL_MEASUREMENTS: std::cell::RefCell<Vec<SettingsControlMeasurement>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(super) fn reset_settings_layout_measurements() {
    SETTINGS_CARD_MEASUREMENTS.with(|measurements| measurements.borrow_mut().clear());
    SETTINGS_CONTROL_MEASUREMENTS.with(|measurements| measurements.borrow_mut().clear());
}

#[cfg(test)]
pub(super) fn settings_card_measurements() -> Vec<SettingsControlMeasurement> {
    SETTINGS_CARD_MEASUREMENTS.with(|measurements| measurements.borrow().clone())
}

#[cfg(test)]
pub(super) fn settings_control_measurements() -> Vec<SettingsControlMeasurement> {
    SETTINGS_CONTROL_MEASUREMENTS.with(|measurements| measurements.borrow().clone())
}

pub(super) fn record_settings_control_box(label: impl Into<String>, rect: egui::Rect) {
    #[cfg(test)]
    SETTINGS_CONTROL_MEASUREMENTS.with(|measurements| {
        measurements.borrow_mut().push(SettingsControlMeasurement {
            label: label.into(),
            width: rect.width(),
            center_x: rect.center().x,
            center_y: rect.center().y,
            right_x: rect.right(),
        });
    });

    #[cfg(not(test))]
    let _ = (label, rect);
}

fn record_settings_card_box(label: impl Into<String>, rect: egui::Rect) {
    #[cfg(test)]
    SETTINGS_CARD_MEASUREMENTS.with(|measurements| {
        measurements.borrow_mut().push(SettingsControlMeasurement {
            label: label.into(),
            width: rect.width(),
            center_x: rect.center().x,
            center_y: rect.center().y,
            right_x: rect.right(),
        });
    });

    #[cfg(not(test))]
    let _ = (label, rect);
}

fn active_settings_control_lane() -> Option<egui::Rect> {
    SETTINGS_CONTROL_LANE.with(|lane| lane.get())
}

struct SettingsControlLaneGuard {
    previous: Option<egui::Rect>,
}

impl SettingsControlLaneGuard {
    fn push(rect: egui::Rect) -> Self {
        let previous = SETTINGS_CONTROL_LANE.with(|lane| {
            let previous = lane.get();
            lane.set(Some(rect));
            previous
        });
        Self { previous }
    }
}

impl Drop for SettingsControlLaneGuard {
    fn drop(&mut self) {
        SETTINGS_CONTROL_LANE.with(|lane| lane.set(self.previous));
    }
}

fn with_settings_control_lane<R>(rect: egui::Rect, add_contents: impl FnOnce() -> R) -> R {
    let _guard = SettingsControlLaneGuard::push(rect);
    add_contents()
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
    heading: &str,
    id_source: &str,
    icon: &str,
    title: &str,
    description: &str,
    default_open: bool,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    category_heading(ui, heading);
    expandable_card(
        ui,
        id_source,
        icon,
        title,
        description,
        default_open,
        add_body,
    );
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
