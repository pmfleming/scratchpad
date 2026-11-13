use super::{
    SettingsUi, available_width_control, choice_text_color, egui, inner_select_row_with_height,
    record_settings_control_box, text_primary, widget_ids,
};

const FRAME_PADDING: f32 = 4.0;
const BUTTON_GAP: f32 = 2.0;
/// Width the wrapped label loses to the selectable button's own padding.
const LABEL_WRAP_INSET: f32 = 20.0;

/// A single framed choice group, trailing-aligned on desktop. When the labels
/// cannot fit, stack the choices and reserve the full height (never overlap rows).
pub(in crate::app::ui::settings) fn segmented_row<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str)],
) -> bool {
    if options.is_empty() {
        return false;
    }
    let original = *value;
    let widths: Vec<_> = options
        .iter()
        .map(|(_, text)| choice_width(ui, text))
        .collect();
    let available = ui.available_width().min(SettingsUi::column_width(ui));
    let layout = SegmentLayout::new(&widths, available, |label_width| {
        choice_height(ui, options, label_width)
    });
    let row_height = layout.height.max(SettingsUi::row_height(ui));
    inner_select_row_with_height(ui, label, None, row_height, |ui| {
        available_width_control(ui, |ui| {
            let lane = ui.max_rect();
            let rect = egui::Rect::from_min_size(
                egui::pos2(
                    lane.right() - layout.width,
                    lane.center().y - layout.height / 2.0,
                ),
                egui::vec2(layout.width, layout.height),
            );
            paint_segment_frame(ui, label, rect);
            let button_rects = layout.button_rects(rect.min, &widths);
            for ((option, text), button_rect) in options.iter().zip(button_rects) {
                widget_ids::rect_scope_with_layout(
                    ui,
                    button_rect,
                    (label, *text),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let selected = *value == *option;
                        if choice_button(ui, label, text, selected, button_rect.size()).clicked() {
                            *value = *option;
                        }
                    },
                );
            }
        });
    });
    *value != original
}

/// Frame size and orientation of a choice group, from its measured label widths.
#[derive(Debug, PartialEq)]
struct SegmentLayout {
    vertical: bool,
    width: f32,
    button_height: f32,
    height: f32,
}

impl SegmentLayout {
    /// `button_height_for` receives the width available to a wrapped label.
    fn new(widths: &[f32], available: f32, button_height_for: impl FnOnce(f32) -> f32) -> Self {
        let gaps = BUTTON_GAP * widths.len().saturating_sub(1) as f32;
        let horizontal_width = widths.iter().sum::<f32>() + gaps + 2.0 * FRAME_PADDING;
        let vertical = horizontal_width > available;
        let width = if vertical {
            widths.iter().copied().fold(0.0, f32::max) + 2.0 * FRAME_PADDING
        } else {
            horizontal_width
        }
        .min(available);
        let button_height = button_height_for(width - 2.0 * FRAME_PADDING - LABEL_WRAP_INSET);
        let buttons_height = if vertical {
            button_height * widths.len() as f32 + gaps
        } else {
            button_height
        };
        Self {
            vertical,
            width,
            button_height,
            height: buttons_height + 2.0 * FRAME_PADDING,
        }
    }

    /// One rect per choice inside a frame whose top-left corner is `frame_min`.
    fn button_rects(&self, frame_min: egui::Pos2, widths: &[f32]) -> Vec<egui::Rect> {
        let mut origin = frame_min + egui::Vec2::splat(FRAME_PADDING);
        let stacked_width = (self.width - 2.0 * FRAME_PADDING).max(0.0);
        widths
            .iter()
            .map(|&button_width| {
                let width = if self.vertical {
                    stacked_width
                } else {
                    button_width
                };
                let rect = egui::Rect::from_min_size(origin, egui::vec2(width, self.button_height));
                if self.vertical {
                    origin.y += self.button_height + BUTTON_GAP;
                } else {
                    origin.x += button_width + BUTTON_GAP;
                }
                rect
            })
            .collect()
    }
}

fn paint_segment_frame(ui: &egui::Ui, label: &str, rect: egui::Rect) {
    ui.painter().rect(
        rect,
        8.0,
        SettingsUi::field_bg(ui),
        egui::Stroke::new(1.0, SettingsUi::card_border(ui)),
        egui::StrokeKind::Inside,
    );
    record_settings_control_box(format!("segments.{label}"), rect);
}

fn choice_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    ui.fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font, text_primary(ui)))
        .size()
        .x
        + 2.0 * ui.spacing().button_padding.x
}

fn choice_button(
    ui: &mut egui::Ui,
    label: &str,
    text: &str,
    selected: bool,
    size: egui::Vec2,
) -> egui::Response {
    let response = widget_ids::surface_response(
        ui,
        (label, text),
        widget_ids::WidgetRole::RadioOption,
        |ui| {
            ui.add_sized(
                size,
                egui::Button::selectable(
                    selected,
                    egui::RichText::new(text).color(choice_text_color(ui, selected)),
                )
                .wrap()
                .corner_radius(5),
            )
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::RadioButton, ui.is_enabled(), selected, text)
    });
    record_settings_control_box(format!("segment.{label}.{text}"), response.rect);
    response
}

fn choice_height<T>(ui: &egui::Ui, options: &[(T, &str)], width: f32) -> f32 {
    options
        .iter()
        .map(|(_, text)| {
            ui.fonts_mut(|fonts| {
                fonts.layout(
                    (*text).to_owned(),
                    egui::TextStyle::Button.resolve(ui.style()),
                    text_primary(ui),
                    width.max(1.0),
                )
            })
            .size()
            .y + 14.0
        })
        .fold(SettingsUi::control_height(ui), f32::max)
}

#[cfg(test)]
mod tests {
    use super::{SegmentLayout, egui};

    #[test]
    fn choices_stay_in_one_row_until_they_overflow_then_stack() {
        let widths = [40.0, 60.0, 50.0];
        let row = SegmentLayout::new(&widths, 200.0, |_| 30.0);
        assert_eq!(
            row,
            SegmentLayout {
                vertical: false,
                width: 162.0,
                button_height: 30.0,
                height: 38.0,
            }
        );
        let stacked = SegmentLayout::new(&widths, 100.0, |label_width| {
            assert_eq!(label_width, 40.0);
            30.0
        });
        assert_eq!(
            stacked,
            SegmentLayout {
                vertical: true,
                width: 68.0,
                button_height: 30.0,
                height: 102.0,
            }
        );

        let rect = |x, y, w| egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, 30.0));
        assert_eq!(
            row.button_rects(egui::Pos2::ZERO, &widths),
            [
                rect(4.0, 4.0, 40.0),
                rect(46.0, 4.0, 60.0),
                rect(108.0, 4.0, 50.0)
            ]
        );
        assert_eq!(
            stacked.button_rects(egui::Pos2::ZERO, &widths),
            [
                rect(4.0, 4.0, 60.0),
                rect(4.0, 36.0, 60.0),
                rect(4.0, 68.0, 60.0)
            ]
        );
    }
}
