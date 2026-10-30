use super::{
    SettingsUi, available_width_control, egui, inner_select_row_with_height,
    record_settings_control_box, text_primary, widget_ids,
};

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
    let font = egui::TextStyle::Button.resolve(ui.style());
    let widths: Vec<_> = options
        .iter()
        .map(|(_, text)| {
            ui.fonts_mut(|fonts| {
                fonts.layout_no_wrap((*text).to_owned(), font.clone(), text_primary(ui))
            })
            .size()
            .x + 2.0 * ui.spacing().button_padding.x
        })
        .collect();
    let available = ui.available_width().min(SettingsUi::CONTROLS.column_width);
    let padding = 4.0;
    let gap = 2.0;
    let horizontal_width =
        widths.iter().sum::<f32>() + gap * (options.len() - 1) as f32 + 2.0 * padding;
    let vertical = horizontal_width > available;
    let width = if vertical {
        widths.iter().copied().fold(0.0, f32::max) + 2.0 * padding
    } else {
        horizontal_width
    }
    .min(available);
    let button_height = SettingsUi::CONTROLS.height;
    let height = 2.0 * padding
        + if vertical {
            button_height * options.len() as f32 + gap * (options.len() - 1) as f32
        } else {
            button_height
        };
    inner_select_row_with_height(ui, label, None, height.max(56.0), |ui| {
        available_width_control(ui, |ui| {
            let lane = ui.max_rect();
            let rect = egui::Rect::from_min_size(
                egui::pos2(lane.right() - width, lane.center().y - height / 2.0),
                egui::vec2(width, height),
            );
            ui.painter().rect(
                rect,
                8.0,
                SettingsUi::field_bg(ui),
                egui::Stroke::new(1.0, SettingsUi::card_border(ui)),
                egui::StrokeKind::Inside,
            );
            record_settings_control_box(format!("segments.{label}"), rect);
            let mut origin = rect.min + egui::Vec2::splat(padding);
            for ((option, text), button_width) in options.iter().zip(&widths) {
                let size = egui::vec2(
                    if vertical {
                        (width - 2.0 * padding).max(0.0)
                    } else {
                        *button_width
                    },
                    button_height,
                );
                let button_rect = egui::Rect::from_min_size(origin, size);
                widget_ids::rect_scope_with_layout(
                    ui,
                    button_rect,
                    (label, *text),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let response = choice_button(ui, label, text, *value == *option, size);
                        if response.clicked() {
                            *value = *option;
                        }
                    },
                );
                if vertical {
                    origin.y += button_height + gap;
                } else {
                    origin.x += button_width + gap;
                }
            }
        });
    });
    *value != original
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
                egui::Button::selectable(selected, text)
                    .truncate()
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
