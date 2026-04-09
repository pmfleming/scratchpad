use eframe::egui;
use serde::{Deserialize, Serialize};
use std::io;

pub const EDITOR_FONT_FAMILY: &str = "scratchpad-editor";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorFontPreset {
    #[default]
    SystemDefault,
    Roboto,
    SpaceMono,
    IBMPlexMono,
}

impl EditorFontPreset {
    pub const ALL: [Self; 4] = [
        Self::SystemDefault,
        Self::Roboto,
        Self::SpaceMono,
        Self::IBMPlexMono,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SystemDefault => "System Default",
            Self::Roboto => "Roboto",
            Self::SpaceMono => "Space Mono",
            Self::IBMPlexMono => "IBM Plex Mono",
        }
    }

    fn font_asset(self) -> Option<(&'static str, &'static [u8])> {
        match self {
            Self::SystemDefault => None,
            Self::Roboto => Some((
                "editor-roboto",
                include_bytes!("../../fonts/Roboto-Regular.ttf"),
            )),
            Self::SpaceMono => Some((
                "editor-space-mono",
                include_bytes!("../../fonts/SpaceMono-Regular.ttf"),
            )),
            Self::IBMPlexMono => Some((
                "editor-ibm-plex-mono",
                include_bytes!("../../fonts/IBMPlexMono-Regular.ttf"),
            )),
        }
    }
}

pub fn apply_editor_fonts(ctx: &egui::Context, preset: EditorFontPreset) -> Result<(), io::Error> {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    let editor_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
    let mut editor_candidates = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let monospace_candidates = fonts
        .families
        .get(&egui::FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    for candidate in monospace_candidates {
        if !editor_candidates.contains(&candidate) {
            editor_candidates.push(candidate);
        }
    }

    if let Some((font_name, font_bytes)) = preset.font_asset() {
        fonts.font_data.insert(
            font_name.to_owned(),
            egui::FontData::from_static(font_bytes).into(),
        );
        editor_candidates.retain(|candidate| candidate != font_name);
        editor_candidates.insert(0, font_name.to_owned());
    }

    fonts.families.insert(editor_family, editor_candidates);
    ctx.set_fonts(fonts);
    Ok(())
}
