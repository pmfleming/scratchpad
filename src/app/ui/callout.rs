use crate::app::chrome::{PhosphorButtonColors, phosphor_button_with_hover_icon_color};
use crate::app::theme::{
    CAPTION_BUTTON_SIZE, CLOSE_HOVER_BG, action_hover_bg, border, text_muted, text_primary,
};
use crate::app::ui::widget_ids;
use eframe::egui;
use std::hash::Hash;

const CALLOUT_RADIUS: u8 = 14;
const CALLOUT_SECTION_RADIUS: u8 = 10;
const SCROLL_BLOCK_HOVER_SECONDS: f64 = 0.25;

pub(crate) fn apply_spacing(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
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
    widget_ids::area(id)
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(true)
        .default_pos(default_position)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.set_min_width(width);
            ui.set_max_width(width);
            let inner = frame(ui).show(ui, add_contents);
            mark_scroll_blocker_if_hovered(ctx, &inner.response);
        });
}

pub(crate) fn mark_scroll_blocker_if_hovered(ctx: &egui::Context, response: &egui::Response) {
    if response.hovered() || response.contains_pointer() {
        let now = ctx.input(|input| input.time);
        ctx.data_mut(|data| data.insert_temp(scroll_blocker_id(), now));
    }
}

pub(crate) fn scroll_blocker_hovered(ctx: &egui::Context) -> bool {
    let now = ctx.input(|input| input.time);
    ctx.data(|data| {
        data.get_temp::<f64>(scroll_blocker_id())
            .is_some_and(|last_hovered| now - last_hovered <= SCROLL_BLOCK_HOVER_SECONDS)
    })
}

pub(crate) fn set_modal_scroll_blocker_active(ctx: &egui::Context, active: bool) {
    ctx.data_mut(|data| {
        if active {
            data.insert_temp(modal_scroll_blocker_id(), true);
        } else {
            data.remove::<bool>(modal_scroll_blocker_id());
        }
    });
}

pub(crate) fn scroll_blocker_active(ctx: &egui::Context) -> bool {
    scroll_blocker_hovered(ctx)
        || ctx.data(|data| data.get_temp::<bool>(modal_scroll_blocker_id()) == Some(true))
}

fn scroll_blocker_id() -> egui::Id {
    widget_ids::ctx_key("callout_scroll_blocker_hover")
}

fn modal_scroll_blocker_id() -> egui::Id {
    widget_ids::ctx_key("callout_scroll_blocker_modal")
}

pub(crate) fn frame(ui: &egui::Ui) -> egui::Frame {
    let popup_frame = egui::Frame::popup(ui.style());

    egui::Frame::NONE
        .fill(popup_frame.fill)
        .stroke(popup_frame.stroke)
        .shadow(popup_frame.shadow)
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
    surface_key: impl Hash + Clone,
    close_tooltip: &str,
    add_leading: impl FnOnce(&mut egui::Ui),
) -> bool {
    let row_width = ui.available_width();
    let row_height = CAPTION_BUTTON_SIZE.y;
    let row_rect = widget_ids::allocate_exact_rect(ui, egui::vec2(row_width, row_height));
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(row_rect.right() - CAPTION_BUTTON_SIZE.x, row_rect.top()),
        CAPTION_BUTTON_SIZE,
    );
    let leading_rect = egui::Rect::from_min_max(
        row_rect.min,
        egui::pos2(
            (close_rect.left() - 6.0).max(row_rect.left()),
            row_rect.bottom(),
        ),
    );

    widget_ids::rect_scope_with_layout(
        ui,
        leading_rect,
        (surface_key.clone(), "leading"),
        egui::Layout::left_to_right(egui::Align::Center),
        add_leading,
    );

    let close_button_key = (surface_key.clone(), "close");

    widget_ids::rect_scope_with_layout(
        ui,
        close_rect,
        (surface_key, "close_cell"),
        *ui.layout(),
        |ui| close_button(ui, close_button_key, close_tooltip).clicked(),
    )
    .inner
}

pub(crate) fn close_button(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    tooltip: &str,
) -> egui::Response {
    phosphor_button_with_hover_icon_color(
        ui,
        surface_key,
        egui_phosphor::regular::X,
        CAPTION_BUTTON_SIZE,
        PhosphorButtonColors::with_hover_icon(
            action_hover_bg(ui),
            CLOSE_HOVER_BG,
            text_primary(ui),
            egui::Color32::WHITE,
        ),
        tooltip,
    )
}

#[derive(Clone, Copy)]
pub(crate) struct IconButtonStyle {
    pub(crate) icon_size: f32,
    pub(crate) size: egui::Vec2,
    pub(crate) fill: egui::Color32,
}

pub(crate) fn icon_button(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    icon: &str,
    style: IconButtonStyle,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(icon)
            .font(egui::FontId::proportional(style.icon_size))
            .color(text(ui)),
    )
    .min_size(style.size)
    .fill(style.fill)
    .stroke(egui::Stroke::new(1.0, border(ui)))
    .corner_radius(egui::CornerRadius::same(8));

    widget_ids::surface_response(ui, surface_key, widget_ids::WidgetRole::IconButton, |ui| {
        ui.add_enabled(enabled, button)
    })
    .on_hover_text(tooltip)
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
