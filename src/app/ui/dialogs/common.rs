use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::theme::CAPTION_BUTTON_SIZE;
use crate::app::ui::callout;
use eframe::egui;
use egui_phosphor::regular::FILE_TEXT;
use std::time::Duration;

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

pub(super) fn relative_age_label(age: Duration) -> String {
    if age < Duration::from_secs(60) {
        format!("{}s", age.as_secs().max(1))
    } else if age < Duration::from_secs(60 * 60) {
        format!("{}m", age.as_secs() / 60)
    } else if age < Duration::from_secs(60 * 60 * 24) {
        format!("{}h", age.as_secs() / (60 * 60))
    } else {
        format!("{}d", age.as_secs() / (60 * 60 * 24))
    }
}

pub(super) fn render_dialog_header(ui: &mut egui::Ui, title: &str) -> bool {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), CAPTION_BUTTON_SIZE.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(FILE_TEXT)
                    .size(16.0)
                    .color(callout::muted_text(ui)),
            );
            ui.add_space(6.0);

            let label_width = (ui.available_width() - CAPTION_BUTTON_SIZE.x - 6.0).max(0.0);
            let label = truncate_dialog_title(ui, title, label_width);
            ui.add_sized(
                egui::vec2(label_width, 0.0),
                egui::Label::new(
                    egui::RichText::new(label)
                        .size(15.0)
                        .monospace()
                        .color(callout::text(ui)),
                ),
            );

            callout::close_button(ui, "Cancel").clicked()
        },
    )
    .inner
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
                icon,
                26.0,
                ICON_CHOICE_BUTTON_SIZE,
                callout::section_fill(ui),
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
    icon: &str,
    label: &str,
    tooltip: &str,
) -> bool {
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
    .on_hover_text(tooltip)
    .clicked()
}
