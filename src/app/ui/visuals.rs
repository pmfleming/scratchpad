use eframe::egui;
pub(crate) fn with_visual_overrides<R>(
    ui: &mut egui::Ui,
    configure: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let previous_visuals = ui.visuals().clone();
    configure(ui);
    let result = add_contents(ui);
    *ui.visuals_mut() = previous_visuals;
    result
}
