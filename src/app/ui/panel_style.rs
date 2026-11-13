use crate::app::{theme, ui::widget_ids};
use eframe::egui;
use std::hash::Hash;

pub(crate) mod window;

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
    icon_button_with_hint(ui, key, icon, label, label, enabled)
}

/// An icon button whose tooltip says more than its accessible name, such as a shortcut.
pub(crate) fn icon_button_with_hint(
    ui: &mut egui::Ui,
    key: impl Hash,
    icon: &str,
    label: &str,
    hint: &str,
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
        response.on_hover_text(hint)
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
    badge(ui, rect, "NOW");
}

/// A small accent tag, such as History's NOW.
pub(crate) fn badge(ui: &egui::Ui, rect: egui::Rect, label: &str) {
    ui.painter()
        .rect_filled(rect, 0, theme::tab_selected_accent(ui));
    text(
        ui,
        rect.shrink2(egui::vec2(4.0, 0.0)),
        label,
        10.0,
        on_accent(ui),
    );
}

/// Width of a `badge` holding `label`.
pub(crate) fn badge_width(ui: &egui::Ui, label: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(label.to_owned(), font(ui, 10.0), on_accent(ui))
        .size()
        .x
        + 8.0
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

/// Keeps arrow keys and Escape with a focused list instead of moving egui focus.
pub(crate) fn lock_navigation_focus(ctx: &egui::Context, id: egui::Id) {
    ctx.memory_mut(|m| {
        m.set_focus_lock_filter(
            id,
            egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                tab: false,
                escape: true,
            },
        );
    });
}

/// Popup and menu style matching the panels: square, bordered, no shadow, and
/// the list selection colour for hovered rows.
pub(crate) fn menu_style(fill: egui::Color32) -> egui::style::StyleModifier {
    egui::style::StyleModifier::new(move |style| {
        egui::containers::menu::menu_style(style);
        let border = theme::border_for_visuals(&style.visuals);
        let hover = theme::tab_selected_bg_for_visuals(&style.visuals);
        let visuals = &mut style.visuals;
        visuals.menu_corner_radius = egui::CornerRadius::ZERO;
        visuals.popup_shadow = egui::Shadow::NONE;
        visuals.window_fill = fill;
        visuals.panel_fill = fill;
        visuals.window_stroke = egui::Stroke::new(1.0, border);
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::ZERO;
        }
        for widget in [
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.bg_fill = hover;
            widget.weak_bg_fill = hover;
        }
    })
}

/// A menu row label: icon, text and, right-aligned, its shortcut or value.
pub(crate) fn paint_menu_row(
    ui: &egui::Ui,
    rect: egui::Rect,
    icon: Option<&str>,
    label: &str,
    trailing: Option<&str>,
    enabled: bool,
) {
    const ICON_X: f32 = 12.0;
    const LABEL_X: f32 = 36.0;
    let size = egui::TextStyle::Button.resolve(ui.style()).size;
    let color = if enabled {
        theme::text_primary(ui)
    } else {
        theme::text_muted(ui).gamma_multiply(0.7)
    };
    let inner = rect;
    if let Some(icon) = icon {
        text(
            ui,
            egui::Rect::from_min_size(
                inner.min + egui::vec2(ICON_X, 0.0),
                egui::vec2(18.0, inner.height()),
            ),
            icon,
            size,
            if enabled {
                theme::text_muted(ui)
            } else {
                color
            },
        );
    }
    let mut right = inner.right() - 10.0;
    if let Some(trailing) = trailing {
        let width = ui
            .painter()
            .layout_no_wrap(trailing.to_owned(), font(ui, 11.0), color)
            .size()
            .x;
        let tail = egui::Rect::from_min_max(
            egui::pos2(right - width, inner.top()),
            egui::pos2(right, inner.bottom()),
        );
        text(ui, tail, trailing, 11.0, theme::text_muted(ui));
        right = tail.left() - 8.0;
    }
    text(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(
                inner.left() + if icon.is_some() { LABEL_X } else { ICON_X },
                inner.top(),
            ),
            egui::pos2(right, inner.bottom()),
        ),
        label,
        size,
        color,
    );
}

/// Transparent menu rows that take the list selection colour on hover; square corners.
pub(crate) fn flat_rows(ui: &mut egui::Ui) {
    let hover = theme::tab_selected_bg(ui);
    let widgets = &mut ui.visuals_mut().widgets;
    widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    for state in [&mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
        state.bg_fill = hover;
        state.weak_bg_fill = hover;
    }
    for state in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        state.bg_stroke = egui::Stroke::NONE;
        state.corner_radius = egui::CornerRadius::ZERO;
    }
}
