use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::ui::callout;
use eframe::egui;
use std::time::Duration;

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
