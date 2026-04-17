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
    #[serde(alias = "slab")]
    Serif,
}

impl EditorFontPreset {
    pub const ALL: [Self; 4] = [Self::Standard, Self::Flex, Self::Mono, Self::Serif];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Flex => "Flex",
            Self::Mono => "Mono",
            Self::Serif => "Serif",
        }
    }

    fn font_asset(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Standard => (
                "editor-noto-sans-display",
                include_bytes!("../../fonts/NotoSansDisplay-Regular.ttf"),
            ),
            Self::Flex => (
                "editor-noto-sans-flex",
                include_bytes!("../../fonts/NotoSans-VF.ttf"),
            ),
            Self::Mono => (
                "editor-noto-sans-mono",
                include_bytes!("../../fonts/NotoSansMono-Regular.ttf"),
            ),
            Self::Serif => (
                "editor-noto-serif-display",
                include_bytes!("../../fonts/NotoSerifDisplay-Regular.ttf"),
            ),
        }
    }
}

const CJK_FONT_ASSETS: [(&str, &[u8]); 4] = [
    (
        "editor-noto-cjk-jp",
        include_bytes!("../../fonts/NotoSansCJKjp-Regular.otf"),
    ),
    (
        "editor-noto-cjk-kr",
        include_bytes!("../../fonts/NotoSansCJKkr-Regular.otf"),
    ),
    (
        "editor-noto-cjk-sc",
        include_bytes!("../../fonts/NotoSansCJKsc-Regular.otf"),
    ),
    (
        "editor-noto-cjk-tc",
        include_bytes!("../../fonts/NotoSansCJKtc-Regular.otf"),
    ),
];

fn insert_font(
    fonts: &mut egui::FontDefinitions,
    font_name: &'static str,
    font_bytes: &'static [u8],
) {
    fonts.font_data.insert(
        font_name.to_owned(),
        egui::FontData::from_static(font_bytes).into(),
    );
}

pub fn apply_editor_fonts(ctx: &egui::Context, preset: EditorFontPreset) -> Result<(), io::Error> {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    let (font_name, font_bytes) = preset.font_asset();
    insert_font(&mut fonts, font_name, font_bytes);
    for (fallback_name, fallback_bytes) in CJK_FONT_ASSETS {
        insert_font(&mut fonts, fallback_name, fallback_bytes);
    }

    let editor_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
    let fallback_names = CJK_FONT_ASSETS.iter().map(|(name, _)| (*name).to_owned());
    let proportional_candidates: Vec<String> = std::iter::once(font_name.to_owned())
        .chain(fallback_names.clone())
        .chain(std::iter::once("phosphor".to_owned()))
        .collect();
    let monospace_candidates: Vec<String> = std::iter::once(font_name.to_owned())
        .chain(fallback_names)
        .collect();

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
