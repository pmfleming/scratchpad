use super::{SettingsUi, egui, widgets::record_settings_control_box};
use std::sync::Arc;

#[cfg(test)]
mod tests;

// Match individual controls as well as category keywords. Keep aliases next to
// the visible label so searches such as "font size" identify Size, not Font.
const SETTINGS: &[(&str, &str, &str)] = &[
    ("appearance", "Use appearance from", "system OS scratchpad"),
    ("appearance", "Font", "typeface family"),
    ("appearance", "Size", "font size points pt"),
    (
        "appearance",
        "Colors",
        "colours theme palette light dark custom system",
    ),
    ("appearance", "Text", "text colour color foreground"),
    ("appearance", "Background", "background colour color"),
    (
        "appearance",
        "Search highlight",
        "search highlight colour color",
    ),
    ("editors", "Word wrap", "wrapping lines"),
    ("editors", "Editor padding", "gutter margins"),
    ("editors", "Indent with", "indentation spaces tabs"),
    ("editors", "Indent width", "indentation spaces columns"),
    ("editors", "Show tabs", "tab characters guides tablines"),
    ("workspace", "Tabs", "tab position top bottom left right"),
    ("workspace", "Auto-hide tabs", "automatic hiding"),
    ("workspace", "Hide after", "auto hide tabs delay seconds"),
    ("workspace", "Status bar", "status bar visibility"),
    ("workspace", "Open files in", "opening new current tab"),
    (
        "workspace",
        "Place new tabs",
        "placement start end before after active tab",
    ),
    ("workspace", "Show recent files", "recent files"),
    (
        "history",
        "Restore history when reopening files",
        "recovery reopen archives",
    ),
    (
        "history",
        "Also open changed disk files",
        "disk version alongside recovered text",
    ),
    (
        "history",
        "Memory per file",
        "undo RAM per file budget limits MiB",
    ),
    (
        "history",
        "Memory across all files",
        "undo RAM aggregate total budget limits MiB",
    ),
    ("application", "On launch", "startup continue session fresh"),
    (
        "application",
        "Settings file",
        "TOML configuration loaded startup open file",
    ),
    (
        "application",
        "Reset all settings",
        "restore preferences defaults",
    ),
];

#[derive(Clone, Default)]
pub(super) struct Search {
    terms: Vec<String>,
    matches: Vec<usize>,
}

impl Search {
    pub(super) fn new(query: &str) -> Self {
        let terms: Vec<_> = query
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|term| !term.is_empty())
            .map(str::to_lowercase)
            .collect();
        let mut matches = Vec::new();
        let mut exact_short_matches = Vec::new();
        for (index, (category, label, aliases)) in SETTINGS.iter().enumerate() {
            let text = format!("{category} {label} {aliases}").to_lowercase();
            if terms.iter().all(|term| text.contains(term)) {
                matches.push(index);
                if terms.iter().all(|term| {
                    term.chars().count() != 2
                        || text
                            .split(|ch: char| !ch.is_alphanumeric())
                            .any(|word| word == term)
                }) {
                    exact_short_matches.push(index);
                }
            }
        }
        // Prefer meaningful short words like OS or pt over substrings in
        // unrelated labels, while still allowing incremental partial searches.
        if !exact_short_matches.is_empty() {
            matches = exact_short_matches;
        }
        Self { terms, matches }
    }

    pub(super) fn matches_category(&self, category: &str, keywords: &str) -> bool {
        let keywords = keywords.to_lowercase();
        self.matches
            .iter()
            .any(|&index| SETTINGS[index].0 == category)
            || self.terms.iter().all(|term| keywords.contains(term))
    }

    fn highlights(&self, label: &str) -> bool {
        self.terms.iter().any(|term| term.chars().count() >= 2)
            && self.matches.len() * 2 < SETTINGS.len()
            && self.matches.iter().any(|&index| SETTINGS[index].1 == label)
    }

    pub(super) fn install(self, ui: &egui::Ui) {
        ui.data_mut(|data| {
            data.insert_temp(egui::Id::unique("settings.search.matches"), Arc::new(self))
        });
    }
}

pub(super) fn is_highlighted(ui: &egui::Ui, label: &str) -> bool {
    ui.data(|data| data.get_temp::<Arc<Search>>(egui::Id::unique("settings.search.matches")))
        .is_some_and(|search| search.highlights(label))
}

pub(super) fn highlight_row(ui: &mut egui::Ui, label: &str, render: impl FnOnce(&mut egui::Ui)) {
    let marker = is_highlighted(ui, label).then(|| ui.painter().add(egui::Shape::Noop));
    let start = ui.cursor().min;
    let width = ui.available_width();
    render(ui);
    if let Some(marker) = marker {
        let bottom = ui.min_rect().bottom().max(ui.cursor().top());
        let rect = egui::Rect::from_min_max(start, egui::pos2(start.x + width, bottom));
        ui.painter().set(
            marker,
            egui::epaint::RectShape::new(
                rect,
                6.0,
                crate::app::theme::tab_selected_bg(ui),
                egui::Stroke::new(1.0, SettingsUi::accent(ui)),
                egui::StrokeKind::Inside,
            ),
        );
        record_settings_control_box(format!("search_match.{label}"), rect);
    }
}
