pub mod variants;
pub use variants::*;

const FONT_NAME: &str = "phosphor";
pub const FONT_FAMILY_NAME: &str = "phosphor-icons";

#[must_use]
pub fn font_family() -> egui::FontFamily {
    egui::FontFamily::Name(FONT_FAMILY_NAME.into())
}

#[must_use]
pub fn font_id(size: f32) -> egui::FontId {
    egui::FontId::new(size, font_family())
}

pub fn add_to_fonts(fonts: &mut egui::FontDefinitions, variant: Variant) {
    // Keep icons out of text fallback chains: Phosphor includes ASCII mappings, and its
    // private-use codepoints can overlap OS fonts such as Nerd Fonts.
    fonts
        .font_data
        .insert(FONT_NAME.into(), variant.font_data().into());
    fonts.families.insert(font_family(), vec![FONT_NAME.into()]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_font_is_isolated_from_interface_font_fallbacks() {
        let mut fonts = egui::FontDefinitions::default();
        let proportional_before = fonts.families[&egui::FontFamily::Proportional].clone();

        add_to_fonts(&mut fonts, Variant::Regular);

        assert_eq!(
            fonts.families[&egui::FontFamily::Proportional],
            proportional_before
        );
        assert_eq!(fonts.families[&font_family()], [FONT_NAME]);
    }

    #[test]
    fn dedicated_family_renders_phosphor_icons() {
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        add_to_fonts(&mut fonts, Variant::Regular);
        ctx.set_fonts(fonts);

        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.fonts_mut(|fonts| {
                assert!(fonts.has_glyph(&font_id(16.0), regular::GEAR.chars().next().unwrap()));
            });
        });
        output.textures_delta.clear();
    }
}
