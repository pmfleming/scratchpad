use super::{ScratchpadApp, SettingsUi, egui, text_muted, text_primary};
use crate::app::ui::widget_ids;
use std::{hash::Hash, ops::RangeInclusive};

mod card;
mod layout_state;
mod primitives;
mod segmented;
pub(super) use card::{render_preview_panel, settings_card_frame};
#[cfg(test)]
pub(super) use layout_state::{
    SettingsControlMeasurement, reset_settings_layout_measurements, settings_card_measurements,
    settings_control_measurements,
};
use layout_state::{active_settings_control_lane, with_settings_control_lane};
pub(super) use segmented::segmented_row;

pub(super) fn record_settings_control_box(label: impl Into<String>, rect: egui::Rect) {
    layout_state::record_settings_control_box(label, rect);
}

/// The only framed level in settings. Searching temporarily reveals a category
/// without changing the user's saved expansion state.
pub(super) fn category(
    ui: &mut egui::Ui,
    key: &str,
    title: &str,
    subtitle: &str,
    reveal: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    let id = widget_ids::local(ui, key);
    let open = reveal
        || ui
            .data_mut(|data| data.get_persisted::<bool>(id))
            .unwrap_or(true);
    settings_card_frame(ui, |ui| {
        let response = widget_ids::surface_response(
            ui,
            key,
            widget_ids::WidgetRole::SettingsCardHeader,
            |ui| {
                ui.add_sized(
                    egui::vec2(ui.available_width(), 44.0),
                    egui::Button::new("").frame(false),
                )
            },
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::DisclosureTriangle, true, title));
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_expanded(open));
        let rect = response.rect;
        ui.painter().text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            if open {
                egui_phosphor::regular::CARET_DOWN
            } else {
                egui_phosphor::regular::CARET_RIGHT
            },
            egui_phosphor::font_id(16.0),
            SettingsUi::accent(),
        );
        ui.painter().text(
            rect.left_center() + egui::vec2(28.0, 0.0),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(20.0),
            text_primary(ui),
        );
        if rect.width() > 560.0 {
            ui.painter().text(
                rect.right_center(),
                egui::Align2::RIGHT_CENTER,
                subtitle,
                egui::FontId::proportional(12.5),
                text_muted(ui),
            );
        }
        if response.clicked() && !reveal {
            ui.data_mut(|data| data.insert_persisted(id, !open));
        }
        if open {
            ui.separator();
            body(ui);
        }
    });
}

pub(super) fn group_heading(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(title)
            .size(12.5)
            .strong()
            .color(text_muted(ui)),
    );
}

pub(super) fn inner_select_row(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    control: impl FnOnce(&mut egui::Ui),
) {
    inner_select_row_with_height(ui, label, description, 56.0, control);
}

fn inner_select_row_with_height(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    control_height: f32,
    control: impl FnOnce(&mut egui::Ui),
) {
    let width = ui.available_width();
    let stacked = width < 540.0;
    let control_width = width.min(SettingsUi::CONTROLS.column_width);
    let label_width = if stacked {
        width
    } else {
        width - control_width - SettingsUi::CONTROLS.gap
    };
    let add_label = |ui: &mut egui::Ui| {
        let mut text = egui::text::LayoutJob::default();
        text.append(
            label,
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(15.0),
                color: text_primary(ui),
                ..Default::default()
            },
        );
        if let Some(description) = description {
            text.append(
                &format!("\n{description}"),
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::proportional(12.5),
                    color: text_muted(ui),
                    ..Default::default()
                },
            );
        }
        text.wrap.max_width = label_width;
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(text));
        let height = if stacked {
            galley.size().y
        } else {
            galley.size().y.max(control_height)
        };
        let response = widget_ids::allocate_exact_rect_interact(
            ui,
            egui::vec2(label_width, height),
            ("settings.label", label),
            egui::Sense::hover(),
            "settings_label",
        );
        let position = egui::pos2(
            response.rect.left(),
            response.rect.center().y - galley.size().y / 2.0,
        );
        record_settings_control_box(
            format!("label.{label}"),
            egui::Rect::from_min_size(position, galley.size()),
        );
        ui.painter().galley(position, galley, text_primary(ui));
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, label));
        if let Some(description) = description {
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_description(description));
        }
    };
    let add_control = |ui: &mut egui::Ui| {
        let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(control_width, control_height));
        record_settings_control_box(format!("inner_select_row.{label}"), rect);
        widget_ids::rect_scope_with_layout(
            ui,
            rect,
            ("settings.row", label),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(control_width);
                ui.name_inputs(label, |ui| with_settings_control_lane(rect, || control(ui)));
            },
        );
    };
    ui.add_space(6.0);
    if stacked {
        add_label(ui);
        add_control(ui);
    } else {
        ui.horizontal(|ui| {
            add_label(ui);
            ui.add_space((SettingsUi::CONTROLS.gap - ui.spacing().item_spacing.x * 2.0).max(0.0));
            add_control(ui);
        });
    }
    ui.add_space(6.0);
}

pub(super) fn available_width_control(ui: &mut egui::Ui, control: impl FnOnce(&mut egui::Ui)) {
    let rect = active_settings_control_lane().unwrap_or_else(|| ui.available_rect_before_wrap());
    widget_ids::rect_scope_with_layout(
        ui,
        rect,
        "settings.control",
        egui::Layout::left_to_right(egui::Align::Center),
        control,
    );
}

pub(super) fn toggle_select_row(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    key: impl Hash,
    current: bool,
    on_change: impl FnOnce(bool),
) {
    let mut value = current;
    inner_select_row(ui, label, description, |ui| {
        available_width_control(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let response = primitives::toggle_switch(ui, key, &mut value);
                record_settings_control_box("toggle_control.switch", response.rect);
            });
        });
    });
    if value != current {
        on_change(value);
    }
}

pub(super) enum DropdownPlacement {
    Leading(f32),
    Trailing(f32),
}

/// Font/Size explicitly share a leading edge; other selects align with actions.
pub(super) fn dropdown<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    key: &str,
    value: &mut T,
    options: &[T],
    label: impl Fn(T) -> String,
    placement: DropdownPlacement,
    unit: &str,
) -> bool {
    let original = *value;
    let (width, leading) = match placement {
        DropdownPlacement::Leading(width) => (width, true),
        DropdownPlacement::Trailing(width) => (width, false),
    };
    available_width_control(ui, |ui| {
        let lane = ui.max_rect();
        let unit_width = if unit.is_empty() {
            0.0
        } else {
            ui.fonts_mut(|fonts| {
                fonts.layout_no_wrap(
                    unit.to_owned(),
                    egui::FontId::proportional(12.5),
                    text_muted(ui),
                )
            })
            .size()
            .x + ui.spacing().item_spacing.x
        };
        let width = width.min((lane.width() - unit_width).max(0.0));
        let left = if leading {
            lane.left()
        } else {
            lane.right() - width - unit_width
        };
        let rect = egui::Rect::from_min_max(egui::pos2(left, lane.top()), lane.max);
        widget_ids::rect_scope_with_layout(
            ui,
            rect,
            (key, "dropdown"),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                let response = widget_ids::combo_box(ui, key)
                    .selected_text(label(*value))
                    .width(width.min(ui.available_width()))
                    .truncate()
                    .show_ui(ui, |ui| {
                        for &option in options {
                            if ui.selectable_value(value, option, label(option)).clicked() {
                                ui.close();
                            }
                        }
                    })
                    .response;
                record_settings_control_box(format!("combo.{key}"), response.rect);
                if !unit.is_empty() {
                    ui.label(egui::RichText::new(unit).size(12.5).color(text_muted(ui)));
                }
            },
        );
    });
    *value != original
}

/// Track / exact value / short unit have identical columns for every slider.
/// Neither widget clamps an existing value merely because settings are rendered.
pub(super) fn numeric_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u64,
    range: RangeInclusive<u64>,
    unit: &str,
) -> bool {
    let mut changed = false;
    inner_select_row(ui, label, None, |ui| {
        available_width_control(ui, |ui| {
            let lane = ui.max_rect();
            let width = lane.width().min(256.0);
            let start = lane.right() - width;
            let number_width = 78.0;
            let unit_width = 32.0;
            let gap = 10.0;
            let track_width = (width - number_width - unit_width - gap * 2.0).max(20.0);
            let slider_rect = egui::Rect::from_min_size(
                egui::pos2(start, lane.top()),
                egui::vec2(track_width, lane.height()),
            );
            widget_ids::rect_scope_with_layout(
                ui,
                slider_rect,
                (label, "slider"),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().slider_width = track_width;
                    let response = ui.add(
                        egui::Slider::new(value, range.clone())
                            .step_by(1.0)
                            .clamping(egui::SliderClamping::Edits)
                            .show_value(false),
                    );
                    changed |= response.changed();
                    record_settings_control_box(format!("slider.{label}"), response.rect);
                },
            );
            let number_rect = egui::Rect::from_min_size(
                egui::pos2(start + track_width + gap, lane.top()),
                egui::vec2(number_width, lane.height()),
            );
            widget_ids::rect_scope_with_layout(
                ui,
                number_rect,
                (label, "number"),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let response = ui.add_sized(
                        egui::vec2(number_width, SettingsUi::CONTROLS.height),
                        egui::DragValue::new(value)
                            .range(range)
                            .clamp_existing_to_range(false)
                            .update_while_editing(false)
                            .speed(1),
                    );
                    changed |= response.changed();
                    record_settings_control_box(format!("number.{label}"), response.rect);
                },
            );
            let unit_rect = egui::Rect::from_min_size(
                egui::pos2(number_rect.right() + gap, lane.top()),
                egui::vec2(unit_width, lane.height()),
            );
            ui.painter().text(
                unit_rect.left_center(),
                egui::Align2::LEFT_CENTER,
                unit,
                egui::FontId::proportional(12.5),
                text_muted(ui),
            );
        });
    });
    changed
}

pub(super) fn action_row(ui: &mut egui::Ui, label: &str, button: &str, help: Option<&str>) -> bool {
    let mut clicked = false;
    inner_select_row(ui, label, help, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = ui.button(button);
            record_settings_control_box(format!("button.{button}"), response.rect);
            clicked = response.clicked();
        });
    });
    clicked
}
