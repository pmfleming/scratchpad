use super::{ScratchpadApp, SettingsUi, egui, widgets::category};
use egui_phosphor::regular::{ARROWS_IN_LINE_VERTICAL, ARROWS_OUT_LINE_VERTICAL};

struct Category {
    key: &'static str,
    title: &'static str,
    subtitle: &'static str,
    search: &'static str,
    render: fn(&mut egui::Ui, &mut ScratchpadApp),
}

const CATEGORIES: [Category; 6] = [
    Category {
        key: "appearance",
        title: "Appearance",
        subtitle: "Text and colors",
        search: "system OS font size theme text background highlight palette",
        render: super::text_formatting::render_appearance,
    },
    Category {
        key: "editors",
        title: "Editors",
        subtitle: "Writing and indentation",
        search: "word wrap padding gutter indent width spaces tabs characters guides preview",
        render: super::text_formatting::render_editing,
    },
    Category {
        key: "search",
        title: "Search",
        subtitle: "Results and replace",
        search: "search result limit matches unlimited default scope preserve case recent searches review threshold files changed",
        render: super::search_options::render,
    },
    Category {
        key: "workspace",
        title: "Workspace",
        subtitle: "Tabs and files",
        search: "position top bottom left right auto hide delay status bar opening new current tab placement recent files",
        render: super::appearance::render_workspace,
    },
    Category {
        key: "history",
        title: "History",
        subtitle: "Recovery and storage",
        search: "recovery retained documents closed tabs branches Now changes undo memory cache per-file total recommended saving fails failure automatic disk usage payloads database journal settings reusable references",
        render: super::recovery::render_recovery_category,
    },
    Category {
        key: "application",
        title: "Application",
        subtitle: "Startup and configuration",
        search: "startup launch session continue fresh TOML settings file configuration reset defaults",
        render: super::opening::render_application,
    },
];

pub(super) fn render_settings_categories(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let query = search_query(ui);
    let search = super::search::Search::new(&query);
    search.clone().install(ui);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            expand_collapse_buttons(ui, query.is_empty());
        });
    });
    apply_expand_all_request(ui);
    let mut visible = 0;
    for item in CATEGORIES
        .iter()
        .filter(|item| matches_category(item, &search))
    {
        visible += 1;
        ui.add_space(SettingsUi::LAYOUT.section_gap);
        category(
            ui,
            item.key,
            item.title,
            item.subtitle,
            !query.is_empty(),
            |ui| (item.render)(ui, app),
        );
    }
    if visible == 0 {
        ui.label("No matching settings. Try tabs, font or history.");
    }
    super::search::Search::default().install(ui);
}

/// The search field's normalised text, kept across frames.
fn search_query(ui: &mut egui::Ui) -> String {
    let search_id = egui::Id::unique("settings.search");
    let mut query = ui
        .data_mut(|data| data.get_temp::<String>(search_id))
        .unwrap_or_default();
    super::widgets::search_field(ui, &mut query);
    let normalised = query.trim().to_lowercase();
    ui.data_mut(|data| data.insert_temp(search_id, query));
    normalised
}

fn expand_all_request_id() -> egui::Id {
    egui::Id::unique("settings.expand_all")
}

/// "Collapse all" and "Expand all", disabled while a search reveals matches.
fn expand_collapse_buttons(ui: &mut egui::Ui, enabled: bool) {
    for (label, icon, open) in [
        ("Collapse all", ARROWS_IN_LINE_VERTICAL, false),
        ("Expand all", ARROWS_OUT_LINE_VERTICAL, true),
    ] {
        let response = super::widget_ids::surface_response(
            ui,
            ("settings.categories", open),
            super::widget_ids::WidgetRole::IconButton,
            |ui| {
                ui.add_enabled(
                    enabled,
                    egui::Button::new(egui::RichText::new(icon).font(egui_phosphor::font_id(18.0)))
                        .frame(false)
                        .min_size(egui::vec2(36.0, SettingsUi::control_height(ui))),
                )
            },
        );
        if response
            .accessible_name(label)
            .on_hover_text(label)
            .clicked()
        {
            // Resolve IDs in the parent scope, after this horizontal row.
            ui.ctx()
                .data_mut(|data| data.insert_temp(expand_all_request_id(), open));
        }
    }
}

fn apply_expand_all_request(ui: &mut egui::Ui) {
    let Some(open) = ui.data_mut(|data| data.remove_temp::<bool>(expand_all_request_id())) else {
        return;
    };
    for item in &CATEGORIES {
        let id = super::widget_ids::local(ui, item.key);
        ui.data_mut(|data| data.insert_persisted(id, open));
    }
}

fn matches_category(category: &Category, search: &super::search::Search) -> bool {
    let haystack = format!(
        "{} {} {}",
        category.title, category.subtitle, category.search
    );
    search.matches_category(category.key, &haystack)
}

#[cfg(test)]
mod tests {
    use super::{CATEGORIES, matches_category};

    #[test]
    fn search_supports_labels_and_legacy_aliases() {
        for (query, key) in [
            ("auto-hide", "workspace"),
            ("gutter", "editors"),
            ("branches", "history"),
            ("search result limit", "search"),
            ("unlimited", "search"),
            ("failure", "history"),
            ("font size", "appearance"),
            ("toml", "application"),
        ] {
            let search = super::super::search::Search::new(query);
            let matches: Vec<_> = CATEGORIES
                .iter()
                .filter(|item| matches_category(item, &search))
                .map(|item| item.key)
                .collect();
            assert_eq!(matches, vec![key], "{query}");
        }
    }
}
