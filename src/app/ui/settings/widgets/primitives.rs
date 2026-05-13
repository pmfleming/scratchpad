use super::{SettingsUi, border, egui, text_muted};
use crate::app::ui::widget_ids;
use std::hash::Hash;
use std::path::Path;

pub(super) fn toggle_switch(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    value: &mut bool,
) -> egui::Response {
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

pub(super) fn value_pill(
    ui: &mut egui::Ui,
    text: &str,
    hover_text: Option<&str>,
    width: f32,
) -> egui::Response {
    let margin = SettingsUi::MARGINS.value_pill_inner;
    let outer_width = width.max(0.0);
    let outer_height = ui.spacing().interact_size.y + (margin.top as f32) + (margin.bottom as f32);
    let response = widget_ids::allocate_exact_rect_interact(
        ui,
        egui::vec2(outer_width, outer_height),
        ("settings.value_pill", text, widget_ids::WidgetRole::Label),
        egui::Sense::hover(),
        "settings_value_pill",
    );

    ui.painter().rect(
        response.rect,
        egui::CornerRadius::same(8),
        SettingsUi::control_bg(ui),
        egui::Stroke::new(1.0, border(ui).gamma_multiply(0.75)),
        egui::StrokeKind::Inside,
    );

    let text_rect = response.rect.shrink2(egui::vec2(
        (margin.left + margin.right) as f32 * 0.5,
        (margin.top + margin.bottom) as f32 * 0.5,
    ));
    ui.painter().with_clip_rect(text_rect).text(
        text_rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::FontId::proportional(SettingsUi::TYPOGRAPHY.description),
        text_muted(ui),
    );
    if let Some(hover_text) = hover_text {
        return response.on_hover_text(hover_text);
    }
    response
}

pub(super) fn path_tail_text(path: &Path, component_count: usize) -> String {
    let components: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();

    if components.len() <= component_count {
        return path.display().to_string();
    }

    let separator = std::path::MAIN_SEPARATOR.to_string();
    let tail = components[components.len() - component_count..].join(&separator);
    format!("...{separator}{tail}")
}
