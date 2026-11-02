use super::{
    ScratchpadApp, SettingsUi, egui,
    widgets::{category, numeric_slider},
};
use egui_phosphor::regular::{ARROWS_IN_LINE_VERTICAL, ARROWS_OUT_LINE_VERTICAL};

struct Category {
    key: &'static str,
    title: &'static str,
    subtitle: &'static str,
    search: &'static str,
    render: fn(&mut egui::Ui, &mut ScratchpadApp),
}

const CATEGORIES: [Category; 5] = [
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
        search: "recovery reopening archives closed tabs changed files retention days expiry budgets limits undo memory RAM persisted saved per-file total recommended automatic disk usage payloads database journal settings reusable references",
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
    let search_id = egui::Id::unique("settings.search");
    let mut query = ui
        .data_mut(|data| data.get_temp::<String>(search_id))
        .unwrap_or_default();
    let response = ui.add_sized(
        egui::vec2(ui.available_width(), 34.0),
        egui::TextEdit::singleline(&mut query).hint_text("Find a setting…"),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Search settings"));
    ui.data_mut(|data| data.insert_temp(search_id, query.clone()));
    let query = query.trim().to_lowercase();
    let mut visible = 0;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                            query.is_empty(),
                            egui::Button::new(
                                egui::RichText::new(icon).font(egui_phosphor::font_id(18.0)),
                            )
                            .frame(false)
                            .min_size(egui::Vec2::splat(SettingsUi::CONTROLS.height)),
                        )
                    },
                );
                if response
                    .accessible_name(label)
                    .on_hover_text(label)
                    .clicked()
                {
                    // Resolve IDs in the parent scope, after this horizontal row.
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::unique("settings.expand_all"), open)
                    });
                }
            }
        });
    });
    if let Some(open) =
        ui.data_mut(|data| data.remove_temp::<bool>(egui::Id::unique("settings.expand_all")))
    {
        for item in &CATEGORIES {
            let id = super::widget_ids::local(ui, item.key);
            ui.data_mut(|data| data.insert_persisted(id, open));
        }
    }
    for item in &CATEGORIES {
        if !matches_category(item, &query) {
            continue;
        }
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
}

fn matches_category(category: &Category, query: &str) -> bool {
    let haystack = format!(
        "{} {} {}",
        category.title, category.subtitle, category.search
    )
    .to_lowercase();
    query
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .all(|term| haystack.contains(term))
}

pub(super) fn text_history_budget(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let original = app.state.app_settings.history.budget;
    let mut budget = original;
    byte_budget_row(
        ui,
        "Memory per file",
        &mut budget.per_file_byte_budget,
        1,
        1024,
    );
    byte_budget_row(
        ui,
        "Memory across all files",
        &mut budget.aggregate_byte_budget,
        4,
        4096,
    );
    byte_budget_row(
        ui,
        "Saved undo per file",
        &mut budget.persisted_payload_budget,
        0,
        1024,
    );
    if budget != original {
        crate::app::app_state::settings_controller::set_history_budget(app, budget);
    }
}

pub(super) fn byte_budget_row(ui: &mut egui::Ui, label: &str, bytes: &mut u64, min: u64, max: u64) {
    const MIB: u64 = 1024 * 1024;
    let mut value = *bytes / MIB;
    if numeric_slider(ui, label, &mut value, min..=max, "MiB") {
        *bytes = value * MIB;
    }
}

#[cfg(test)]
mod tests {
    use super::{CATEGORIES, matches_category};

    #[test]
    fn search_supports_labels_and_legacy_aliases() {
        for (query, key) in [
            ("auto-hide", "workspace"),
            ("gutter", "editors"),
            ("persisted per-file", "history"),
            ("font size", "appearance"),
            ("toml", "application"),
        ] {
            let matches: Vec<_> = CATEGORIES
                .iter()
                .filter(|item| matches_category(item, query))
                .map(|item| item.key)
                .collect();
            assert_eq!(matches, vec![key], "{query}");
        }
    }
}
