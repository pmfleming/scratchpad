pub mod variants;
pub use variants::*;

pub fn add_to_fonts(fonts: &mut egui::FontDefinitions, variant: Variant) {
    fonts
        .font_data
        .insert("phosphor".into(), variant.font_data().into());

    if let Some(font_keys) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        font_keys.insert(1, "phosphor".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_font_claims_icons_but_not_interface_text() {
        const PHOSPHOR_ONLY: &str = "phosphor-only";

        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        add_to_fonts(&mut fonts, Variant::Regular);
        fonts.families.insert(
            egui::FontFamily::Name(PHOSPHOR_ONLY.into()),
            vec!["phosphor".into()],
        );
        ctx.set_fonts(fonts);

        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let font_id = egui::FontId::new(16.0, egui::FontFamily::Name(PHOSPHOR_ONLY.into()));
            ui.fonts_mut(|fonts| {
                assert!(fonts.has_glyph(&font_id, regular::GEAR.chars().next().unwrap()));
                assert!((' '..='~').all(|character| !fonts.has_glyph(&font_id, character)));
            });
        });
        output.textures_delta.clear();
    }
}
