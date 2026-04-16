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
