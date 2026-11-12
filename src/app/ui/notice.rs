use eframe::egui;
pub(crate) fn show(ui: &mut egui::Ui, glyph: &str, title: &str, detail: &str) {
    ui.horizontal_top(|ui| {
        ui.label(
            egui::RichText::new(glyph)
                .font(egui_phosphor::font_id(16.0))
                .color(crate::app::theme::text_muted(ui)),
        );
        ui.vertical(|ui| {
            ui.label(title);
            if !detail.is_empty() {
                ui.label(
                    egui::RichText::new(detail)
                        .size(12.5)
                        .color(crate::app::theme::text_muted(ui)),
                );
            }
        });
    });
}
