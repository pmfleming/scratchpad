use super::{egui, format_bytes, text_muted, text_primary, widget_ids};
use crate::app::ui::settings::widgets::record_settings_control_box;
use std::sync::Arc;

pub(super) fn usage_row(ui: &mut egui::Ui, label: &str, bytes: u64, help: &str) {
    usage_row_with_marker(ui, label, bytes, help, None);
}

/// Explicit dot / label / value columns avoid add_sized's centered label
/// placement. Only labels wrap; exact formatted values keep a common right edge.
pub(super) fn usage_row_with_marker(
    ui: &mut egui::Ui,
    label: &str,
    bytes: u64,
    help: &str,
    marker: Option<egui::Color32>,
) {
    let value = format_bytes(bytes);
    let value_galley = ui.fonts_mut(|fonts| {
        fonts.layout_no_wrap(
            value.clone(),
            egui::FontId::proportional(13.0),
            text_primary(ui),
        )
    });
    let marker_width = if marker.is_some() { 19.0 } else { 0.0 };
    let label_width = (ui.available_width() - marker_width - value_galley.size().x - 12.0).max(0.0);
    let label_galley = ui.fonts_mut(|fonts| {
        fonts.layout(
            label.to_owned(),
            egui::FontId::proportional(12.5),
            text_muted(ui),
            label_width,
        )
    });
    let height = 24.0_f32
        .max(label_galley.size().y)
        .max(value_galley.size().y);
    let row = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), height));
    if let Some(color) = marker {
        let rect = egui::Rect::from_center_size(
            egui::pos2(row.left() + 5.0, row.center().y),
            egui::Vec2::splat(10.0),
        );
        ui.painter().circle_filled(rect.center(), 5.0, color);
        record_settings_control_box(format!("legend.marker.{label}"), rect);
        widget_ids::interact(
            ui,
            rect,
            widget_ids::local(ui, ("storage.marker", label)),
            egui::Sense::hover(),
            "storage_marker",
        )
        .on_hover_text(help);
    }
    let label_rect = egui::Rect::from_min_size(
        egui::pos2(
            row.left() + marker_width,
            row.center().y - label_galley.size().y / 2.0,
        ),
        label_galley.size(),
    );
    let value_rect = egui::Rect::from_min_size(
        egui::pos2(
            row.right() - value_galley.size().x,
            row.center().y - value_galley.size().y / 2.0,
        ),
        value_galley.size(),
    );
    paint_label(ui, label_rect, label_galley, (label, "label"), label, help);
    paint_label(
        ui,
        value_rect,
        value_galley,
        (label, "value"),
        &value,
        &format!("{bytes} bytes\n{help}"),
    );
    record_settings_control_box(format!("usage.label.{label}"), label_rect);
    record_settings_control_box(format!("usage.value.{label}"), value_rect);
}

fn paint_label(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    galley: Arc<egui::Galley>,
    key: (&str, &str),
    text: &str,
    help: &str,
) {
    ui.painter().galley(rect.min, galley, text_primary(ui));
    let response = widget_ids::interact(
        ui,
        rect,
        widget_ids::local(ui, ("storage.text", key)),
        egui::Sense::hover(),
        "storage_text",
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), text));
    response.on_hover_text(help);
}
