use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::ui::callout;
use crate::app::ui::settings::dialog_card_frame;
use crate::app::ui::widget_ids;
use eframe::egui;
use egui_phosphor::regular::FILE_TEXT;
use std::hash::Hash;

pub(super) const ICON_CHOICE_BUTTON_SIZE: egui::Vec2 = egui::vec2(72.0, 54.0);

pub(super) fn show_callout(
    ctx: &egui::Context,
    id: &'static str,
    position: egui::Pos2,
    width: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    callout::show_floating(ctx, id, position, width, add_contents);
}

pub(super) fn show_centered_callout(
    ctx: &egui::Context,
    id: &'static str,
    size: egui::Vec2,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    show_callout(
        ctx,
        id,
        callout::centered_position(ctx, size),
        size.x,
        add_contents,
    );
}

pub(super) fn apply_editor_dialog_typography(ui: &mut egui::Ui) {
    let font_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
    let style = ui.style_mut();
    style.override_font_id = Some(egui::FontId::new(15.0, font_family.clone()));
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(15.0, font_family.clone()),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, font_family.clone()),
    );
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::new(12.0, font_family));
}

pub(super) fn render_dialog_header(ui: &mut egui::Ui, title: &str) -> bool {
    callout::header_row(ui, ("dialog_header", title), "Cancel", |ui| {
        ui.label(
            egui::RichText::new(FILE_TEXT)
                .size(16.0)
                .color(callout::muted_text(ui)),
        );
        ui.add_space(6.0);

        let label_width = (ui.available_width() - 6.0).max(0.0);
        let label = truncate_dialog_title(ui, title, label_width);
        let label_response = widget_ids::surface_response(
            ui,
            ("dialog_header.title", title),
            widget_ids::WidgetRole::Label,
            |ui| {
                ui.add_sized(
                    egui::vec2(label_width, 0.0),
                    egui::Label::new(
                        egui::RichText::new(&label)
                            .size(15.0)
                            .monospace()
                            .color(callout::text(ui)),
                    ),
                )
            },
        );
        if label != title {
            label_response.on_hover_text(title);
        }
    })
}

pub(super) fn history_dialog_card<R>(
    ui: &mut egui::Ui,
    corner_radius: u8,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let width = ui.available_width();
    let content_width = (width - 24.0).max(0.0);
    dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(corner_radius))
        .show(ui, |ui| {
            ui.set_width(content_width);
            ui.set_min_width(content_width);
            ui.set_max_width(content_width);
            add_contents(ui)
        })
        .inner
}

pub(super) fn history_dialog_header(
    ui: &mut egui::Ui,
    id_source: impl Hash + Clone,
    close_tooltip: &str,
    title: &str,
    title_size: f32,
) -> bool {
    callout::header_row(ui, id_source, close_tooltip, |ui| {
        ui.label(
            egui::RichText::new(title)
                .size(title_size)
                .color(callout::text(ui)),
        );
    })
}

fn truncate_dialog_title(ui: &egui::Ui, title: &str, max_width: f32) -> String {
    let marker = "...";
    let font_id = egui::FontId::monospace(15.0);

    if text_width(ui, title, font_id.clone()) <= max_width {
        return title.to_owned();
    }
    if text_width(ui, marker, font_id.clone()) >= max_width {
        return marker.to_owned();
    }

    let chars = title.chars().collect::<Vec<_>>();
    let mut prefix_len = chars.len().saturating_sub(1);

    loop {
        let prefix = chars[..prefix_len].iter().collect::<String>();
        let candidate = format!("{prefix}{marker}");

        if text_width(ui, &candidate, font_id.clone()) <= max_width {
            return candidate;
        }

        if prefix_len > 1 {
            prefix_len -= 1;
        } else {
            return marker.to_owned();
        }
    }
}

fn text_width(ui: &egui::Ui, text: &str, font_id: egui::FontId) -> f32 {
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font_id, callout::text(ui))
            .size()
            .x
    })
}

pub(super) fn render_icon_choice_dialog<T: Copy, const N: usize>(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    close_requested: &mut bool,
    actions: [(&str, &str, T); N],
) -> Option<T> {
    callout::apply_spacing(ui);
    ui.spacing_mut().item_spacing = egui::vec2(10.0, 12.0);

    if render_dialog_header(ui, title) {
        *close_requested = true;
    }

    ui.add_space(2.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(subtitle)
                .size(12.0)
                .color(callout::muted_text(ui)),
        );
    });

    ui.add_space(2.0);

    let mut selected = None;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
        for (icon, tooltip, action) in actions {
            if callout::icon_button(
                ui,
                ("icon_choice", title, tooltip),
                icon,
                callout::IconButtonStyle {
                    icon_size: 26.0,
                    size: ICON_CHOICE_BUTTON_SIZE,
                    fill: callout::section_fill(ui),
                },
                tooltip,
                true,
            )
            .clicked()
            {
                selected = Some(action);
            }
        }
    });
    selected
}

pub(super) fn render_dialog_action_button(
    ui: &mut egui::Ui,
    surface_key: impl Hash,
    icon: &str,
    label: &str,
    tooltip: &str,
) -> bool {
    widget_ids::surface_response(
        ui,
        surface_key,
        widget_ids::WidgetRole::ActionButton,
        |ui| {
            ui.add(
                egui::Button::new(
                    egui::RichText::new(format!("{icon} {label}"))
                        .size(12.0)
                        .color(callout::text(ui)),
                )
                .fill(callout::section_fill(ui))
                .corner_radius(egui::CornerRadius::same(8))
                .min_size(egui::vec2(98.0, 34.0)),
            )
        },
    )
    .on_hover_text(tooltip)
    .clicked()
}
