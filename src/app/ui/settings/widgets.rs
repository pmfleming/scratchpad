use super::{ScratchpadApp, SettingsUi, egui, text_muted, text_primary};
use crate::app::ui::widget_ids;
use std::hash::Hash;
mod numeric;
pub(super) use numeric::numeric_slider;

mod card;
mod layout_state;
mod primitives;
pub(super) use primitives::{color_button, destructive_button};
mod segmented;
pub(super) use card::{render_preview_panel, settings_card_frame};
#[cfg(test)]
pub(super) use layout_state::{
    SettingsControlMeasurement, reset_settings_layout_measurements, settings_card_measurements,
    settings_control_measurements,
};
use layout_state::{active_settings_control_lane, with_settings_control_lane};
pub(super) use segmented::segmented_row;

/// Text selection keeps its own foreground; the focus frame uses the accent.
pub(super) fn search_field(ui: &mut egui::Ui, query: &mut String) -> egui::Response {
    let response = ui.add_sized(
        egui::vec2(ui.available_width(), SettingsUi::control_height(ui)),
        egui::TextEdit::singleline(query).hint_text("Find a setting…"),
    );
    if response.has_focus() && ui.is_enabled() {
        ui.painter().rect_stroke(
            response.rect,
            7.0,
            egui::Stroke::new(2.0, SettingsUi::accent(ui)),
            egui::StrokeKind::Inside,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::TextInput, ui.is_enabled(), "Search settings")
    });
    response
}

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
        let response = category_heading(ui, key, title, subtitle, open);
        if response.clicked() && !reveal {
            ui.data_mut(|data| data.insert_persisted(id, !open));
        }
        if open {
            ui.separator();
            body(ui);
        }
    });
}

fn category_heading(
    ui: &mut egui::Ui,
    key: &str,
    title: &str,
    subtitle: &str,
    open: bool,
) -> egui::Response {
    let width = ui.available_width();
    let title_galley = ui.fonts_mut(|fonts| {
        fonts.layout(
            title.to_owned(),
            SettingsUi::font(ui, 20.0),
            text_primary(ui),
            (width - 28.0).max(1.0),
        )
    });
    let subtitle_galley = ui.fonts_mut(|fonts| {
        fonts.layout_no_wrap(
            subtitle.to_owned(),
            SettingsUi::font(ui, 12.5),
            text_muted(ui),
        )
    });
    let height = (44.0 * SettingsUi::scale(ui).max(1.0)).max(title_galley.size().y + 12.0);
    let response =
        widget_ids::surface_response(ui, key, widget_ids::WidgetRole::SettingsCardHeader, |ui| {
            ui.add_sized(
                egui::vec2(width, height),
                egui::Button::new("").frame(false),
            )
        });
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::DisclosureTriangle, true, title));
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
        SettingsUi::accent(ui),
    );
    let title_pos = egui::pos2(
        rect.left() + 28.0,
        rect.center().y - title_galley.size().y / 2.0,
    );
    let title_width = title_galley.size().x;
    ui.painter()
        .galley(title_pos, title_galley, text_primary(ui));
    if width > 560.0 && title_width + subtitle_galley.size().x + 52.0 < width {
        let pos = egui::pos2(
            rect.right() - subtitle_galley.size().x,
            rect.center().y - subtitle_galley.size().y / 2.0,
        );
        ui.painter().galley(pos, subtitle_galley, text_muted(ui));
    }
    response
}

pub(super) fn group_heading(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(title)
            .font(SettingsUi::font(ui, 12.5))
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
    inner_select_row_with_height(ui, label, description, SettingsUi::row_height(ui), control);
}

fn inner_select_row_with_height(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    control_height: f32,
    control: impl FnOnce(&mut egui::Ui),
) {
    super::search::highlight_row(ui, label, |ui| {
        render_select_row(ui, label, description, control_height, control);
    });
}

fn render_select_row(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    control_height: f32,
    control: impl FnOnce(&mut egui::Ui),
) {
    let width = ui.available_width();
    let stacked = width < 540.0 * SettingsUi::scale(ui).max(1.0);
    let control_width = width.min(SettingsUi::column_width(ui));
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
                font_id: egui::TextStyle::Body.resolve(ui.style()),
                color: text_primary(ui),
                ..Default::default()
            },
        );
        if let Some(description) = description {
            text.append(
                &format!("\n{description}"),
                0.0,
                egui::TextFormat {
                    font_id: SettingsUi::font(ui, 12.5),
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

/// Right-align a select and optional unit, honoring the caller's width in points.
pub(super) fn dropdown<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    key: &str,
    value: &mut T,
    options: &[T],
    label: impl Fn(T) -> String,
    width: f32,
    unit: &str,
) -> bool {
    let original = *value;
    available_width_control(ui, |ui| {
        let lane = ui.max_rect();
        let unit_width = if unit.is_empty() {
            0.0
        } else {
            ui.fonts_mut(|fonts| {
                fonts.layout_no_wrap(unit.to_owned(), SettingsUi::font(ui, 12.5), text_muted(ui))
            })
            .size()
            .x + ui.spacing().item_spacing.x
        };
        let width = width.min((lane.width() - unit_width).max(0.0));
        let left = lane.right() - width - unit_width;
        let rect = egui::Rect::from_min_max(egui::pos2(left, lane.top()), lane.max);
        widget_ids::rect_scope_with_layout(
            ui,
            rect,
            (key, "dropdown"),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                let popup_style = (**ui.style()).clone();
                let response = widget_ids::combo_box(ui, key)
                    .selected_text(label(*value))
                    .width(width.min(ui.available_width()))
                    .truncate()
                    .popup_style(popup_style.into())
                    .show_ui(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        for &option in options {
                            let text =
                                egui::RichText::new(label(option)).color(if *value == option {
                                    ui.visuals().selection.stroke.color
                                } else {
                                    text_primary(ui)
                                });
                            if ui.selectable_value(value, option, text).clicked() {
                                ui.close();
                            }
                        }
                    })
                    .response;
                record_settings_control_box(format!("combo.{key}"), response.rect);
                let mut control_rect = response.rect;
                if !unit.is_empty() {
                    let unit = ui.label(
                        egui::RichText::new(unit)
                            .font(SettingsUi::font(ui, 12.5))
                            .color(text_muted(ui)),
                    );
                    record_settings_control_box(format!("unit.{key}"), unit.rect);
                    control_rect = control_rect.union(unit.rect);
                }
                record_settings_control_box(format!("dropdown.{key}"), control_rect);
            },
        );
    });
    *value != original
}

pub(super) fn action_row(ui: &mut egui::Ui, label: &str, button: &str, help: Option<&str>) -> bool {
    action_row_with(ui, label, button, help, |ui, text| ui.button(text))
}

pub(super) fn destructive_action_row(
    ui: &mut egui::Ui,
    label: &str,
    button: &str,
    help: Option<&str>,
) -> bool {
    action_row_with(ui, label, button, help, destructive_button)
}

fn action_row_with(
    ui: &mut egui::Ui,
    label: &str,
    button: &str,
    help: Option<&str>,
    add_button: impl FnOnce(&mut egui::Ui, &str) -> egui::Response,
) -> bool {
    let mut clicked = false;
    let width = ui.available_width().min(SettingsUi::column_width(ui));
    let button_height =
        ui.fonts_mut(|fonts| {
            fonts.layout(
                button.to_owned(),
                egui::TextStyle::Button.resolve(ui.style()),
                text_primary(ui),
                (width - 20.0).max(1.0),
            )
        })
        .size()
        .y + 14.0;
    inner_select_row_with_height(
        ui,
        label,
        help,
        SettingsUi::row_height(ui).max(button_height),
        |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let response = add_button(ui, button);
                record_settings_control_box(format!("button.{button}"), response.rect);
                clicked = response.clicked();
            });
        },
    );
    clicked
}
