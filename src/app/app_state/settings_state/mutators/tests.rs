use super::ScratchpadApp;
use crate::app::app_state::settings_controller;
use crate::app::app_state::test_support::{
    named_tabs, test_app as app_with_root, test_app_with_tabs,
};

#[test]
fn command_open_settings_selects_only_settings_slot() {
    let mut app = test_app(["one.txt", "two.txt"]);
    crate::app::app_state::workspace::display_tabs::select_only_tab_slot(&mut app, 0);
    crate::app::app_state::workspace::display_tabs::toggle_tab_slot_selection(&mut app, 1);

    settings_controller::open_settings(&mut app);

    assert!(crate::app::app_state::settings_state::showing_settings(
        &app
    ));
    assert_eq!(
        selected_slots(&app),
        vec![crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&app)]
    );
    assert!(
        crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
            &app,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&app)
        )
    );
}

#[test]
fn tab_strip_open_settings_can_preserve_existing_selection() {
    let mut app = test_app(["one.txt", "two.txt"]);
    crate::app::app_state::workspace::display_tabs::select_only_tab_slot(&mut app, 0);
    crate::app::app_state::workspace::display_tabs::toggle_tab_slot_selection(&mut app, 1);

    settings_controller::open_settings_preserving_tab_selection(&mut app);

    assert!(crate::app::app_state::settings_state::showing_settings(
        &app
    ));
    assert_eq!(
        selected_slots(&app),
        vec![
            0,
            1,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&app)
        ]
    );
}

#[test]
fn close_settings_selects_only_active_workspace_slot() {
    let mut app = test_app(["one.txt", "two.txt"]);
    settings_controller::open_settings_preserving_tab_selection(&mut app);
    crate::app::app_state::workspace::display_tabs::toggle_tab_slot_selection(&mut app, 0);

    settings_controller::close_settings(&mut app);

    assert!(!crate::app::app_state::settings_state::showing_settings(
        &app
    ));
    assert_eq!(
        selected_slots(&app),
        vec![crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&app)]
    );
    assert!(
        !crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
            &app,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&app)
        )
    );
}

#[test]
fn startup_restores_the_active_surface_from_the_session() {
    for show_settings in [false, true] {
        let root = temp_root();
        let mut app = app_with_root(&root);
        assert!(crate::app::app_state::settings_state::settings_tab_open(
            &app
        ));
        if show_settings {
            settings_controller::open_settings(&mut app);
        }
        assert_eq!(
            crate::app::app_state::settings_state::showing_settings(&app),
            show_settings
        );
        crate::app::services::session_manager::persist_session_now(&mut app).unwrap();

        drop(app); // A restart must release the single database owner.
        let mut restored = app_with_root(&root);
        restored.set_session_persist_on_drop(false);

        assert!(crate::app::app_state::settings_state::settings_tab_open(
            &restored
        ));
        assert_eq!(
            crate::app::app_state::settings_state::showing_settings(&restored),
            show_settings
        );
        let active_slot =
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&restored);
        assert_eq!(
            crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
                &restored,
                active_slot
            ),
            show_settings
        );
        assert_eq!(selected_slots(&restored), vec![active_slot]);
    }
}

#[test]
fn reset_settings_to_defaults_restores_startup_default_state() {
    let mut app = test_app(["custom.txt"]);
    settings_controller::close_settings(&mut app);

    settings_controller::reset_settings_to_defaults(&mut app);

    assert!(crate::app::app_state::settings_state::showing_settings(
        &app
    ));
    assert_eq!(
        app.state.app_settings.tab_list_position(),
        crate::app::services::settings_store::TabListPosition::Top
    );
    assert_eq!(
        app.state.app_settings.theme_mode(),
        crate::app::services::settings_store::AppThemeMode::Auto
    );
    assert_eq!(app.tab_manager.tabs.as_slice().len(), 1);
    assert_eq!(
        app.tab_manager.tabs.as_slice()[0].buffers.buffer.name,
        "Untitled"
    );
    assert!(
        crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
            &app,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(&app)
        )
    );
}

fn test_app<const N: usize>(names: [&str; N]) -> ScratchpadApp {
    test_app_with_tabs(&temp_root(), named_tabs(&names))
}

fn temp_root() -> std::path::PathBuf {
    tempfile::tempdir().expect("create temp app root").keep()
}

fn selected_slots(app: &ScratchpadApp) -> Vec<usize> {
    app.state.workspace_selection.selected_slots().collect()
}

#[test]
fn search_limit_changes_persist_and_restart_an_open_query() {
    use crate::app::services::settings_store::SearchResultLimit;
    let mut app = test_app(["one.txt"]);
    app.open_search();
    app.set_search_query("needle");
    let generation = app.state.search_state.runtime.requested_generation;
    settings_controller::set_search_result_limit(
        &mut app,
        SearchResultLimit::from_matches(250_000),
    );
    assert!(app.state.search_state.runtime.requested_generation > generation);
    let stored = app
        .state
        .persistence
        .settings_store
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.search.result_limit,
        SearchResultLimit::from_matches(250_000)
    );
    settings_controller::reset_settings_to_defaults(&mut app);
    assert_eq!(
        app.state.app_settings.search.result_limit,
        SearchResultLimit::Recommended
    );
}

mod appearance {
    use super::{settings_controller, test_app};
    use crate::app::app_state::ScratchpadApp;
    use crate::app::fonts::EditorFontPreset;
    use crate::app::services::settings_store::{AppThemeMode, EditorAppearanceSource};
    use eframe::egui;

    const RED: egui::Color32 = egui::Color32::from_rgb(200, 30, 30);

    fn colors(app: &ScratchpadApp) -> [egui::Color32; 3] {
        let settings = &app.state.app_settings;
        [
            settings.editor_text_color(),
            settings.editor_background_color(),
            settings.editor_text_highlight_color(),
        ]
    }

    /// Settings as the next start would see them.
    fn reloaded(app: &ScratchpadApp) -> ScratchpadApp {
        let stored = app
            .state
            .persistence
            .settings_store
            .load()
            .unwrap()
            .unwrap();
        let mut next = test_app([]);
        crate::app::app_state::settings_state::apply_settings(&mut next, stored);
        next
    }

    #[test]
    fn editing_while_following_the_system_forks_from_what_is_on_screen() {
        let mut app = test_app([]);
        settings_controller::set_editor_appearance_source(&mut app, EditorAppearanceSource::System);
        let font = app.state.app_settings.editor_font_selection();
        let size = app.state.app_settings.font_size();
        let [_, background, highlight] = colors(&app);

        settings_controller::set_editor_text_color(&mut app, RED);

        let settings = &app.state.app_settings;
        assert_eq!(
            settings.editor_appearance_source(),
            EditorAppearanceSource::App
        );
        assert_eq!(settings.theme_mode(), AppThemeMode::Custom);
        assert_eq!(colors(&app), [RED, background, highlight]);
        assert_eq!(settings.font_size(), size);
        assert_eq!(settings.editor_font_selection(), font);
        assert_eq!(colors(&reloaded(&app)), [RED, background, highlight]);
    }

    #[test]
    fn choosing_a_preset_while_following_the_system_keeps_the_desktop_font() {
        let mut app = test_app([]);
        settings_controller::set_editor_appearance_source(&mut app, EditorAppearanceSource::System);
        let font = app.state.app_settings.editor_font_selection();

        settings_controller::set_theme_mode(&mut app, AppThemeMode::Light);

        assert_eq!(app.state.app_settings.theme_mode(), AppThemeMode::Light);
        assert_eq!(app.state.app_settings.editor_font_selection(), font);
    }

    #[test]
    fn returning_to_scratchpad_without_an_edit_restores_the_saved_set() {
        let mut app = test_app([]);
        settings_controller::set_theme_mode(&mut app, AppThemeMode::Dark);
        settings_controller::set_font_size(&mut app, 20.0);
        settings_controller::set_editor_font(&mut app, Some(EditorFontPreset::Serif));
        let saved = colors(&app);

        settings_controller::set_editor_appearance_source(&mut app, EditorAppearanceSource::System);
        settings_controller::set_editor_appearance_source(&mut app, EditorAppearanceSource::App);

        let settings = &app.state.app_settings;
        assert_eq!(settings.theme_mode(), AppThemeMode::Dark);
        assert_eq!(settings.font_size(), 20.0);
        assert_eq!(settings.editor_font(), EditorFontPreset::Serif);
        assert_eq!(colors(&app), saved);
    }

    #[test]
    fn a_swatch_edit_turns_the_preset_into_a_custom_set_that_survives_a_restart() {
        let mut app = test_app([]);
        settings_controller::set_theme_mode(&mut app, AppThemeMode::Light);
        let [text, background, _] = colors(&app);

        settings_controller::set_editor_text_highlight_color(&mut app, RED);

        assert_eq!(app.state.app_settings.theme_mode(), AppThemeMode::Custom);
        assert_eq!(colors(&app), [text, background, RED]);
        let next = reloaded(&app);
        assert_eq!(next.state.app_settings.theme_mode(), AppThemeMode::Custom);
        assert_eq!(colors(&next), [text, background, RED]);
    }

    #[test]
    fn presets_reset_all_three_swatches() {
        let mut app = test_app([]);
        settings_controller::set_theme_mode(&mut app, AppThemeMode::Dark);
        let stock = colors(&app);
        settings_controller::set_editor_background_color(&mut app, RED);
        settings_controller::set_editor_text_highlight_color(&mut app, RED);

        settings_controller::set_theme_mode(&mut app, AppThemeMode::Dark);

        assert_eq!(app.state.app_settings.theme_mode(), AppThemeMode::Dark);
        assert_eq!(colors(&app), stock);
        assert_eq!(
            reloaded(&app).state.app_settings.theme_mode(),
            AppThemeMode::Dark
        );
    }

    #[test]
    fn choosing_custom_keeps_the_preset_colors_on_screen() {
        let mut app = test_app([]);
        settings_controller::set_theme_mode(&mut app, AppThemeMode::Light);
        let light = colors(&app);

        settings_controller::set_theme_mode(&mut app, AppThemeMode::Custom);

        assert_eq!(app.state.app_settings.theme_mode(), AppThemeMode::Custom);
        assert_eq!(colors(&app), light);
    }

    #[test]
    fn system_font_choice_uses_the_desktop_family() {
        let mut app = test_app([]);
        settings_controller::set_editor_font(&mut app, None);

        let selection = app.state.app_settings.editor_font_selection();
        assert_eq!(selection.source, crate::app::fonts::EditorFontSource::Os);
        assert_eq!(
            selection.os_family,
            crate::app::system_appearance::editor_font_family()
        );
    }
}
