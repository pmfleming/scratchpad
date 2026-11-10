use super::{
    SettingsUi, available_width_control, egui, inner_select_row_with_height,
    record_settings_control_box, text_muted, widget_ids,
};
use std::ops::RangeInclusive;

/// Track / exact value / short unit have identical columns for every slider.
/// Neither widget clamps an existing value merely because settings are rendered.
pub(in crate::app::ui::settings) fn numeric_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u64,
    range: RangeInclusive<u64>,
    unit: &str,
) -> bool {
    let layout = NumericLayout::new(ui, value, &range, unit);
    let mut changed = false;
    inner_select_row_with_height(ui, label, None, layout.height, |ui| {
        available_width_control(ui, |ui| {
            let lane = ui.max_rect();
            let [slider_rect, number_rect, unit_rect] = layout.rects(lane);
            let track_width = slider_rect.width();
            let number_width = number_rect.width();
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
                    changed |= response.changed();
                    record_settings_control_box(format!("slider.{label}"), response.rect);
                },
            );
            widget_ids::rect_scope_with_layout(
                ui,
                number_rect,
                (label, "number"),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let response = widget_ids::surface_response(
                        ui,
                        ("settings.numeric", label),
                        "number",
                        |ui| {
                            ui.add_sized(
                                egui::vec2(number_width, SettingsUi::control_height(ui)),
                                egui::DragValue::new(value)
                                    .range(range)
                                    .clamp_existing_to_range(false)
                                    .update_while_editing(false)
                                    .speed(1),
                            )
                        },
                    );
                    changed |= response.changed();
                    record_settings_control_box(format!("number.{label}"), response.rect);
                },
            );
            ui.painter().text(
                unit_rect.left_center(),
                egui::Align2::LEFT_CENTER,
                unit,
                SettingsUi::font(ui, 12.5),
                text_muted(ui),
            );
        });
    });
    changed
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
