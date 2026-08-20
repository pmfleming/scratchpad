use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io;
use std::sync::OnceLock;

pub const EDITOR_FONT_FAMILY: &str = "scratchpad-editor";
pub const DEFAULT_OS_FONT_LABEL: &str = "Default OS font";
const OS_EDITOR_FONT_NAME: &str = "editor-os-font";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorFontSource {
    #[default]
    Scratchpad,
    Os,
}

impl EditorFontSource {
    pub const ALL: [Self; 2] = [Self::Scratchpad, Self::Os];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Scratchpad => "Scratchpad fonts",
            Self::Os => "OS fonts",
        }
    }
}

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

    #[must_use]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorFontSelection {
    pub source: EditorFontSource,
    pub scratchpad_preset: EditorFontPreset,
    pub os_family: Option<String>,
}

impl EditorFontSelection {
    #[must_use]
    pub fn scratchpad(preset: EditorFontPreset) -> Self {
        Self {
            source: EditorFontSource::Scratchpad,
            scratchpad_preset: preset,
            os_family: None,
        }
    }

    #[must_use]
    pub fn os(os_family: Option<String>, fallback_preset: EditorFontPreset) -> Self {
        Self {
            source: EditorFontSource::Os,
            scratchpad_preset: fallback_preset,
            os_family,
        }
    }

    #[must_use]
    pub fn label(&self) -> String {
        match self.source {
            EditorFontSource::Scratchpad => self.scratchpad_preset.label().to_owned(),
            EditorFontSource::Os => self
                .os_family
                .as_deref()
                .filter(|family| !family.trim().is_empty())
                .unwrap_or(DEFAULT_OS_FONT_LABEL)
                .to_owned(),
        }
    }
}

const FALLBACK_FONT_ASSETS: [(&str, &[u8]); 6] = [
    (
        "editor-scratchpad-control-symbols",
        include_bytes!("../../fonts/ScratchpadControlSymbols-Regular.ttf"),
    ),
    (
        "editor-noto-symbols2",
        include_bytes!("../../fonts/NotoSansSymbols2-Regular.ttf"),
    ),
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
    tab_width: f32,
) {
    fonts.font_data.insert(
        font_name.to_owned(),
        egui::FontData::from_static(font_bytes)
            .tweak(font_tweak(tab_width))
            .into(),
    );
}

fn insert_owned_font(
    fonts: &mut egui::FontDefinitions,
    font_name: &'static str,
    font_bytes: Vec<u8>,
    face_index: u32,
    tab_width: f32,
) {
    let mut font_data = egui::FontData::from_owned(font_bytes).tweak(font_tweak(tab_width));
    font_data.index = face_index;
    fonts
        .font_data
        .insert(font_name.to_owned(), font_data.into());
}

fn font_tweak(tab_width: f32) -> egui::FontTweak {
    egui::FontTweak {
        tab_size: tab_width,
        ..Default::default()
    }
}

pub fn apply_editor_fonts(
    ctx: &egui::Context,
    selection: &EditorFontSelection,
    tab_width: f32,
) -> Result<(), io::Error> {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    let (font_name, warning) = match selection.source {
        EditorFontSource::Scratchpad => {
            let (font_name, font_bytes) = selection.scratchpad_preset.font_asset();
            insert_font(&mut fonts, font_name, font_bytes, tab_width);
            (font_name, None)
        }
        EditorFontSource::Os => match load_os_editor_font(selection.os_family.as_deref()) {
            Ok(os_font) => {
                insert_owned_font(
                    &mut fonts,
                    OS_EDITOR_FONT_NAME,
                    os_font.bytes,
                    os_font.index,
                    tab_width,
                );
                (OS_EDITOR_FONT_NAME, None)
            }
            Err(error) => {
                let (font_name, font_bytes) = selection.scratchpad_preset.font_asset();
                insert_font(&mut fonts, font_name, font_bytes, tab_width);
                (font_name, Some(error))
            }
        },
    };

    for (fallback_name, fallback_bytes) in FALLBACK_FONT_ASSETS {
        insert_font(&mut fonts, fallback_name, fallback_bytes, tab_width);
    }

    let editor_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
    let editor_candidates: Vec<String> = std::iter::once(font_name.to_owned())
        .chain(
            FALLBACK_FONT_ASSETS
                .iter()
                .map(|(name, _)| (*name).to_owned()),
        )
        .collect();

    fonts
        .families
        .insert(egui::FontFamily::Proportional, editor_candidates.clone());
    fonts
        .families
        .insert(egui::FontFamily::Monospace, editor_candidates.clone());
    fonts.families.insert(editor_family, editor_candidates);
    ctx.set_fonts(fonts);

    match warning {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[derive(Clone, Debug)]
struct ResolvedOsFont {
    bytes: Vec<u8>,
    index: u32,
}

fn load_os_editor_font(family: Option<&str>) -> Result<ResolvedOsFont, io::Error> {
    let family = family.map(str::trim).filter(|family| !family.is_empty());
    let db = system_font_database();
    let id = if let Some(family) = family {
        query_named_family(db, family).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("OS font family '{family}' was not found"),
            )
        })?
    } else {
        query_default_os_editor_font(db).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "No default OS editor font was found",
            )
        })?
    };

    db.with_face_data(id, |data, index| ResolvedOsFont {
        bytes: data.to_vec(),
        index,
    })
    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "OS font data could not be loaded"))
}

fn query_named_family(db: &fontdb::Database, family: &str) -> Option<fontdb::ID> {
    db.query(&fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        ..regular_query()
    })
}

fn query_default_os_editor_font(db: &fontdb::Database) -> Option<fontdb::ID> {
    for &family in default_os_editor_font_candidates() {
        if let Some(id) = match family {
            OsFontCandidate::Generic(generic) => db.query(&fontdb::Query {
                families: &[generic],
                ..regular_query()
            }),
            OsFontCandidate::Named(name) => query_named_family(db, name),
        } {
            return Some(id);
        }
    }
    None
}

#[derive(Clone, Copy)]
enum OsFontCandidate<'a> {
    Generic(fontdb::Family<'a>),
    Named(&'a str),
}

fn default_os_editor_font_candidates() -> &'static [OsFontCandidate<'static>] {
    #[cfg(target_os = "windows")]
    {
        &[
            OsFontCandidate::Named("Cascadia Mono"),
            OsFontCandidate::Named("Consolas"),
            OsFontCandidate::Named("Segoe UI"),
            OsFontCandidate::Generic(fontdb::Family::Monospace),
        ]
    }

    #[cfg(target_os = "linux")]
    {
        // On NixOS/Hyprland this asks fontconfig for the configured monospace alias first.
        // The named families are a conservative fallback for non-Nix Linux installs.
        &[
            OsFontCandidate::Generic(fontdb::Family::Monospace),
            OsFontCandidate::Named("Noto Sans Mono"),
            OsFontCandidate::Named("DejaVu Sans Mono"),
            OsFontCandidate::Named("Liberation Mono"),
            OsFontCandidate::Named("Ubuntu Mono"),
        ]
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        &[
            OsFontCandidate::Generic(fontdb::Family::Monospace),
            OsFontCandidate::Generic(fontdb::Family::SansSerif),
        ]
    }
}

fn regular_query() -> fontdb::Query<'static> {
    fontdb::Query {
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        style: fontdb::Style::Normal,
        ..fontdb::Query::default()
    }
}

pub fn available_os_font_families() -> &'static [String] {
    static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
    FAMILIES.get_or_init(|| {
        let mut families = BTreeSet::new();
        for face in system_font_database().faces() {
            for (family, _) in &face.families {
                if !family.trim().is_empty() {
                    families.insert(family.clone());
                }
            }
        }
        families.into_iter().collect()
    })
}

fn system_font_database() -> &'static fontdb::Database {
    static DB: OnceLock<fontdb::Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    })
}

#[cfg(test)]
mod tests {
    use super::{
        EditorFontPreset, EditorFontSelection, EditorFontSource, default_os_editor_font_candidates,
        font_tweak,
    };

    #[test]
    fn configured_indent_width_sets_literal_tab_width() {
        assert_eq!(font_tweak(7.0).tab_size, 7.0);
    }

    #[test]
    fn default_os_font_candidates_are_available() {
        assert!(!default_os_editor_font_candidates().is_empty());
    }

    #[test]
    fn os_font_selection_labels_default_when_no_family_is_set() {
        let selection = EditorFontSelection::os(None, EditorFontPreset::Standard);

        assert_eq!(selection.source, EditorFontSource::Os);
        assert_eq!(selection.label(), "Default OS font");
    }
}
