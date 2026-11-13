use crate::app::{theme, ui::widget_ids};
use eframe::egui;
use std::hash::Hash;

pub(crate) const ROW_HEIGHT: f32 = 30.0;

pub(crate) fn on_accent(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

pub(crate) fn text_end(
    ui: &egui::Ui,
    rect: egui::Rect,
    label: &str,
    size: f32,
    color: egui::Color32,
) {
    let fits = |text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), font(ui, size), color)
            .size()
            .x
            <= rect.width()
    };
    if fits(label) {
        text(ui, rect, label, size, color);
        return;
    }
    let starts: Vec<_> = label
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(label.len()))
        .collect();
    let index = starts.partition_point(|&start| !fits(&format!("…{}", &label[start..])));
    let suffix = starts.get(index).copied().unwrap_or(label.len());
    text(ui, rect, &format!("…{}", &label[suffix..]), size, color);
}

pub(crate) fn text_right(
    ui: &egui::Ui,
    rect: egui::Rect,
    label: &str,
    size: f32,
    color: egui::Color32,
) {
    let width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font(ui, size), color)
        .size()
        .x;
    text(
        ui,
        egui::Rect::from_min_max(
            egui::pos2((rect.right() - width).max(rect.left()), rect.top()),
            rect.max,
        ),
        label,
        size,
        color,
    );
}

pub(crate) fn apply(ui: &mut egui::Ui) {
    let accent = theme::tab_selected_accent(ui);
    let selected = theme::tab_selected_bg(ui);
    let danger = theme::diff_del(ui);
    let style = ui.style_mut();
    style.spacing.item_spacing = egui::vec2(6.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 5.0);
    style.visuals.selection.bg_fill = selected;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, accent);
    style.visuals.error_fg_color = danger;
    style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
    style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
    for widget in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::ZERO;
    }
}

pub(crate) fn icon_button(
    ui: &mut egui::Ui,
    key: impl Hash,
    icon: &str,
    label: &str,
    enabled: bool,
) -> egui::Response {
    widget_ids::scope(ui, key, |ui| {
        ui.spacing_mut().button_padding = egui::Vec2::ZERO;
        let response = ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(icon).font(egui_phosphor::font_id(15.0)))
                .frame(false)
                .min_size(egui::vec2(26.0, 26.0)),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
        response.on_hover_text(label)
    })
    .inner
}

pub(crate) fn text(ui: &egui::Ui, rect: egui::Rect, label: &str, size: f32, color: egui::Color32) {
    if rect.width() <= 0.0 {
        return;
    }
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        if label.chars().count() == 1
            && label
                .chars()
                .all(|c| ('\u{e000}'..='\u{f8ff}').contains(&c))
        {
            egui_phosphor::font_id(size)
        } else {
            font(ui, size)
        },
        color,
    );
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let pos = egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5);
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(pos, galley, color);
}

pub(crate) fn font(ui: &egui::Ui, size: f32) -> egui::FontId {
    let mut font = ui
        .style()
        .override_font_id
        .clone()
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()));
    font.size = size;
    font
}

pub(crate) fn icon_label(
    ui: &egui::Ui,
    icon: &str,
    text: &str,
    size: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let color = theme::text_primary(ui);
    job.append(
        icon,
        0.0,
        egui::TextFormat::simple(egui_phosphor::font_id(size), color),
    );
    job.append(text, 5.0, egui::TextFormat::simple(font(ui, size), color));
    job
}

pub(crate) fn now_badge(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter()
        .rect_filled(rect, 0, theme::tab_selected_accent(ui));
    text(
        ui,
        rect.shrink2(egui::vec2(4.0, 0.0)),
        "NOW",
        10.0,
        on_accent(ui),
    );
}

/// How a rail marker is drawn: the list's emphasis, a filled step, or an open step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RailDot {
    Current,
    Filled,
    Hollow,
}

/// Paints the vertical rail through `rect` at `x` with a marker at `dot_y`.
pub(crate) fn rail(ui: &egui::Ui, rect: egui::Rect, x: f32, dot_y: f32, dot: RailDot) {
    let accent = theme::tab_selected_accent(ui);
    ui.painter().line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        egui::Stroke::new(1.5, accent),
    );
    let center = egui::pos2(x, dot_y);
    let (radius, fill, width) = match dot {
        RailDot::Current => (5.0, theme::text_primary(ui), 2.5),
        RailDot::Filled => (3.0, accent, 1.5),
        RailDot::Hollow => (3.0, ui.visuals().panel_fill, 1.5),
    };
    ui.painter()
        .circle(center, radius, fill, egui::Stroke::new(width, accent));
}

/// The flat, bordered frame shared by History and Search windows.
pub(crate) fn window_frame(ctx: &egui::Context, fill: egui::Color32) -> egui::Frame {
    egui::Frame::NONE
        .fill(fill)
        .stroke(egui::Stroke::new(
            1.0,
            theme::border_for_visuals(&ctx.global_style().visuals),
        ))
        .inner_margin(egui::Margin::symmetric(12, 8))
}

/// Tiling desktops close panels on an outside click and omit their close button.
pub(crate) fn dismiss_on_outside_click(app: &crate::app::app_state::ScratchpadApp) -> bool {
    use crate::app::platform::{PlatformProfile, resolved_profile};
    matches!(
        resolved_profile(app.state.app_settings.platform_profile()),
        PlatformProfile::LinuxGeneric | PlatformProfile::Hyprland
    )
}
