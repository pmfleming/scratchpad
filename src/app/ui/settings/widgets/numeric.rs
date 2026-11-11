use super::{
    SettingsUi, available_width_control, egui, inner_select_row_with_height,
    record_settings_control_box, text_muted, widget_ids,
};
use std::ops::RangeInclusive;

pub(in crate::app::ui::settings) struct ResetButton<'a> {
    pub label: &'a str,
    pub enabled: bool,
    pub tooltip: &'a str,
}

pub(in crate::app::ui::settings) struct NumericFormat {
    pub show: fn(u64) -> String,
    pub parse: fn(&str) -> Option<u64>,
    pub current_text: Option<String>,
}

impl NumericFormat {
    fn text(&self, value: u64, initial: u64) -> String {
        if value == initial
            && let Some(text) = &self.current_text
        {
            text.clone()
        } else {
            (self.show)(value)
        }
    }
}

pub(in crate::app::ui::settings) struct SliderRow<'a> {
    pub range: RangeInclusive<u64>,
    pub unit: &'a str,
    pub reserve_reset: bool,
    pub reset: Option<ResetButton<'a>>,
    pub format: Option<NumericFormat>,
}

#[derive(Default)]
pub(in crate::app::ui::settings) struct SliderResponse {
    pub changed: bool,
    pub reset: bool,
}

/// Track / exact value / short unit have identical columns for every slider.
/// Neither widget clamps an existing value merely because settings are rendered.
pub(in crate::app::ui::settings) fn numeric_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u64,
    range: RangeInclusive<u64>,
    unit: &str,
) -> bool {
    numeric_slider_row(
        ui,
        label,
        value,
        SliderRow {
            range,
            unit,
            reserve_reset: false,
            reset: None,
            format: None,
        },
    )
    .changed
}

pub(in crate::app::ui::settings) fn numeric_slider_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u64,
    options: SliderRow<'_>,
) -> SliderResponse {
    let SliderRow {
        range,
        unit,
        reserve_reset,
        reset,
        format,
    } = options;
    let reset_width = if reserve_reset {
        SettingsUi::control_height(ui) + 10.0
    } else {
        0.0
    };
    let mut layout = NumericLayout::new(ui, value, &range, unit);
    if reserve_reset {
        layout.reserve_reset(ui, reset_width);
    }
    layout.update_rows();
    let mut result = SliderResponse::default();
    let initial_value = *value;
    inner_select_row_with_height(ui, label, None, layout.height, |ui| {
        available_width_control(ui, |ui| {
            let full_lane = ui.max_rect();
            let mut lane = full_lane;
            lane.max.x -= reset_width;
            let [slider_rect, number_rect, unit_rect] = layout.rects(lane);
            let track_width = slider_rect.width();
            widget_ids::rect_scope_with_layout(
                ui,
                slider_rect,
                (label, "slider"),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().slider_width = track_width;
                    SettingsUi::apply_slider_style(ui);
                    // Preview resizing moves this row. Keep drag identity
                    // independent of the surrounding rectangle-based scopes.
                    let response = widget_ids::surface_response(
                        ui,
                        ("settings.numeric", label),
                        "slider",
                        |ui| {
                            ui.add(
                                egui::Slider::new(value, range.clone())
                                    .step_by(1.0)
                                    .clamping(egui::SliderClamping::Edits)
                                    .handle_shape(egui::style::HandleShape::Circle)
                                    .trailing_fill(true)
                                    .show_value(false),
                            )
                        },
                    );
                    result.changed |= response.changed();
                    if let Some(format) = &format {
                        let text = format.text(*value, initial_value);
                        ui.ctx()
                            .accesskit_node_builder(response.id, |node| node.set_value(text));
                    }
                    record_settings_control_box(format!("slider.{label}"), response.rect);
                },
            );
            result.changed |= number_input(
                ui,
                label,
                value,
                range,
                number_rect,
                format.as_ref(),
                initial_value,
            );
            ui.painter().text(
                unit_rect.left_center(),
                egui::Align2::LEFT_CENTER,
                unit,
                SettingsUi::font(ui, 12.5),
                text_muted(ui),
            );
            if let Some(reset) = reset {
                let rect = egui::Rect::from_center_size(
                    egui::pos2(
                        full_lane.right() - SettingsUi::control_height(ui) / 2.0,
                        number_rect.center().y,
                    ),
                    egui::Vec2::splat(SettingsUi::control_height(ui)),
                );
                result.reset = reset_button(ui, label, rect, reset);
            }
        });
    });
    result
}

fn number_input(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u64,
    range: RangeInclusive<u64>,
    rect: egui::Rect,
    format: Option<&NumericFormat>,
    initial_value: u64,
) -> bool {
    widget_ids::rect_scope_with_layout(
        ui,
        rect,
        (label, "number"),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let response =
                widget_ids::surface_response(ui, ("settings.numeric", label), "number", |ui| {
                    let mut number = egui::DragValue::new(&mut *value)
                        .range(range)
                        .clamp_existing_to_range(false)
                        .update_while_editing(false)
                        .speed(1);
                    if let Some(format) = format {
                        number = number
                            .custom_formatter(move |v, _| format.text(v as u64, initial_value))
                            .custom_parser(move |text| (format.parse)(text).map(|v| v as f64));
                    }
                    ui.add_sized(
                        egui::vec2(rect.width(), SettingsUi::control_height(ui)),
                        number,
                    )
                });
            if let Some(format) = format {
                let text = format.text(*value, initial_value);
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_value(text));
            }
            record_settings_control_box(format!("number.{label}"), response.rect);
            response.changed()
        },
    )
    .inner
}

fn reset_button(ui: &mut egui::Ui, label: &str, rect: egui::Rect, reset: ResetButton<'_>) -> bool {
    widget_ids::rect_scope_with_layout(
        ui,
        rect,
        (label, "reset"),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().button_padding = egui::Vec2::ZERO;
            let response =
                widget_ids::surface_response(ui, ("settings.reset", label), "button", |ui| {
                    ui.add_enabled(
                        reset.enabled,
                        egui::Button::new(
                            egui::RichText::new(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                                .font(egui_phosphor::font_id(18.0)),
                        )
                        .min_size(rect.size()),
                    )
                });
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, reset.enabled, reset.label)
            });
            record_settings_control_box(format!("button.{}", reset.label), response.rect);
            let clicked = response.clicked();
            response
                .on_hover_text(reset.tooltip)
                .on_disabled_hover_text(reset.tooltip);
            clicked
        },
    )
    .inner
}

struct NumericLayout {
    width: f32,
    number: f32,
    unit: f32,
    height: f32,
    control_height: f32,
    rows: usize,
}

impl NumericLayout {
    fn reserve_reset(&mut self, ui: &egui::Ui, width: f32) {
        self.width = (self.width - width).max(1.0);
        // Reserve the same value column for every row, including Unlimited.
        let number =
            ui.fonts_mut(|fonts| {
                fonts.layout_no_wrap(
                    "Unlimited".into(),
                    egui::TextStyle::Button.resolve(ui.style()),
                    super::text_primary(ui),
                )
            })
            .size()
            .x + 20.0;
        self.number = self.number.max(number).min(self.width);
    }

    fn update_rows(&mut self) {
        self.rows = if self.number + self.unit + 40.0 <= self.width {
            1
        } else if self.number + self.unit + 10.0 <= self.width {
            2
        } else {
            3
        };
        self.height = self
            .height
            .max(self.control_height * self.rows as f32 + 10.0 * (self.rows - 1) as f32);
    }
    fn new(ui: &egui::Ui, value: &u64, range: &RangeInclusive<u64>, unit: &str) -> Self {
        let width = ui.available_width().min(SettingsUi::column_width(ui));
        let number = [*value, *range.start(), *range.end()]
            .into_iter()
            .map(|n| {
                ui.fonts_mut(|fonts| {
                    fonts.layout_no_wrap(
                        n.to_string(),
                        egui::TextStyle::Button.resolve(ui.style()),
                        super::text_primary(ui),
                    )
                })
                .size()
                .x + 20.0
            })
            .fold(78.0, f32::max);
        let unit = ui
            .fonts_mut(|fonts| {
                fonts.layout_no_wrap(unit.to_owned(), SettingsUi::font(ui, 12.5), text_muted(ui))
            })
            .size()
            .x
            .max(32.0);
        let rows = if number + unit + 40.0 <= width {
            1
        } else if number + unit + 10.0 <= width {
            2
        } else {
            3
        };
        let control_height = SettingsUi::control_height(ui);
        let height =
            SettingsUi::row_height(ui).max(control_height * rows as f32 + 10.0 * (rows - 1) as f32);
        Self {
            width,
            number: number.min(width),
            unit,
            height,
            control_height,
            rows,
        }
    }

    fn rects(&self, lane: egui::Rect) -> [egui::Rect; 3] {
        let rect = |x, y, w, h| egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
        let left = lane.right() - self.width;
        if self.rows == 1 {
            let track = self.width - self.number - self.unit - 20.0;
            [
                rect(left, lane.top(), track, lane.height()),
                rect(left + track + 10.0, lane.top(), self.number, lane.height()),
                rect(
                    lane.right() - self.unit,
                    lane.top(),
                    self.unit,
                    lane.height(),
                ),
            ]
        } else {
            let y = lane.top() + self.control_height + 10.0;
            let number_right = if self.rows == 2 {
                lane.right() - self.unit - 10.0
            } else {
                lane.right()
            };
            let unit_y = if self.rows == 2 {
                y
            } else {
                y + self.control_height + 10.0
            };
            [
                rect(left, lane.top(), self.width, self.control_height),
                rect(
                    number_right - self.number,
                    y,
                    self.number,
                    self.control_height,
                ),
                rect(
                    lane.right() - self.unit,
                    unit_y,
                    self.unit,
                    self.control_height,
                ),
            ]
        }
    }
}
