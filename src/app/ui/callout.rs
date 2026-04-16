use crate::app::chrome::phosphor_button;
use crate::app::theme::{
    CAPTION_BUTTON_SIZE, CLOSE_BG, CLOSE_HOVER_BG, action_bg, action_hover_bg, border, text_muted,
    text_primary,
};
use eframe::egui;

const CALLOUT_RADIUS: u8 = 14;
const CALLOUT_SECTION_RADIUS: u8 = 10;
pub(crate) const CALLOUT_TOP_OFFSET: f32 = 4.0;
pub(crate) const CALLOUT_HORIZONTAL_OFFSET: f32 = 16.0;

pub(crate) fn apply_spacing(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
}

pub(crate) fn top_left_position(ctx: &egui::Context, left_edge: f32) -> egui::Pos2 {
    egui::pos2(
        left_edge + CALLOUT_HORIZONTAL_OFFSET,
        ctx.content_rect().top() + CALLOUT_TOP_OFFSET,
    )
}

pub(crate) fn centered_position(ctx: &egui::Context, size: egui::Vec2) -> egui::Pos2 {
    let rect = ctx.content_rect();
    egui::pos2(
        rect.center().x - (size.x * 0.5),
        rect.center().y - (size.y * 0.5),
    )
}

pub(crate) fn show_floating(
    ctx: &egui::Context,
    id: &'static str,
    default_position: egui::Pos2,
    width: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(true)
        .default_pos(default_position)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.set_min_width(width);
            frame(ui).show(ui, add_contents);
        });
}

pub(crate) fn frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::NONE
        .fill(action_bg(ui))
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(CALLOUT_RADIUS))
        .inner_margin(egui::Margin::symmetric(12, 8))
}

pub(crate) fn section_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::NONE
        .fill(section_fill(ui))
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(CALLOUT_SECTION_RADIUS))
        .inner_margin(egui::Margin::symmetric(10, 8))
}

pub(crate) fn header_row(
    ui: &mut egui::Ui,
    close_tooltip: &str,
    add_leading: impl FnOnce(&mut egui::Ui),
) -> bool {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), CAPTION_BUTTON_SIZE.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            add_leading(ui);

            let trailing_space = (ui.available_width() - CAPTION_BUTTON_SIZE.x).max(0.0);
            if trailing_space > 0.0 {
                ui.add_space(trailing_space);
            }

            close_button(ui, close_tooltip).clicked()
        },
    )
    .inner
}

pub(crate) fn close_button(ui: &mut egui::Ui, tooltip: &str) -> egui::Response {
    phosphor_button(
        ui,
        egui_phosphor::regular::X,
        CAPTION_BUTTON_SIZE,
        CLOSE_BG,
        CLOSE_HOVER_BG,
        tooltip,
    )
}

pub(crate) fn icon_button(
    ui: &mut egui::Ui,
    icon: &str,
    icon_size: f32,
    size: egui::Vec2,
    fill: egui::Color32,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    let button = egui::Button::new(egui::RichText::new(icon).size(icon_size).color(text(ui)))
        .min_size(size)
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(8));

    ui.add_enabled(enabled, button).on_hover_text(tooltip)
}

pub(crate) fn badge(ui: &mut egui::Ui, label: &str) {
    egui::Frame::NONE
        .fill(section_fill(ui))
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(12.0).color(muted_text(ui)));
        });
}

pub(crate) fn section_fill(ui: &egui::Ui) -> egui::Color32 {
    action_hover_bg(ui)
}

pub(crate) fn text(ui: &egui::Ui) -> egui::Color32 {
    text_primary(ui)
}

pub(crate) fn muted_text(ui: &egui::Ui) -> egui::Color32 {
    text_muted(ui)
}
