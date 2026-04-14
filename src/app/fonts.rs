use eframe::egui;
use serde::{Deserialize, Serialize};
use std::io;

pub const EDITOR_FONT_FAMILY: &str = "scratchpad-editor";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorFontPreset {
    #[default]
    Standard,
    Flex,
    Mono,
    Slab,
    Serif,
}

impl EditorFontPreset {
    pub const ALL: [Self; 5] = [
        Self::Standard,
        Self::Flex,
        Self::Mono,
        Self::Slab,
        Self::Serif,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Flex => "Flex",
            Self::Mono => "Mono",
            Self::Slab => "Slab",
            Self::Serif => "Serif",
        }
    }

    fn font_asset(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Standard => (
                "editor-roboto",
                include_bytes!("../../fonts/Roboto-Regular.ttf"),
            ),
            Self::Flex => (
                "editor-roboto-flex",
                include_bytes!("../../fonts/RobotoFlex-Regular.ttf"),
            ),
            Self::Mono => (
                "editor-roboto-mono",
                include_bytes!("../../fonts/RobotoMono-Regular.ttf"),
            ),
            Self::Slab => (
                "editor-roboto-slab",
                include_bytes!("../../fonts/RobotoSlab-Regular.ttf"),
            ),
            Self::Serif => (
                "editor-roboto-serif",
                include_bytes!("../../fonts/RobotoSerif-Regular.ttf"),
            ),
        }
    }
}

pub fn apply_editor_fonts(ctx: &egui::Context, preset: EditorFontPreset) -> Result<(), io::Error> {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    let (font_name, font_bytes) = preset.font_asset();
    fonts.font_data.insert(
        font_name.to_owned(),
        egui::FontData::from_static(font_bytes).into(),
    );

    let editor_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
    let proportional_candidates = vec![font_name.to_owned(), "phosphor".to_owned()];
    let monospace_candidates = vec![font_name.to_owned()];

    fonts.families.insert(
        egui::FontFamily::Proportional,
        proportional_candidates.clone(),
    );
    fonts
        .families
        .insert(egui::FontFamily::Monospace, monospace_candidates.clone());
    fonts.families.insert(editor_family, monospace_candidates);
    ctx.set_fonts(fonts);
    Ok(())
}
