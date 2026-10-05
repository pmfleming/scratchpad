use super::{toggle_active_buffer_control_chars, toggle_active_buffer_reading_order};
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::view::LayoutCacheKey;
use crate::app::domain::{BufferState, SearchHighlightState, SplitAxis, TabManager, WorkspaceTab};
use crate::app::services::{session_store::SessionStore, settings_store::SettingsStore};
use crate::app::startup::StartupOptions;
use eframe::egui;

#[test]
fn display_toggles_affect_only_active_buffer_and_its_views() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(directory.path().join("session")),
        SettingsStore::new(directory.path().join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    let mut tab = WorkspaceTab::untitled();
    let active_view = tab.layout.active_view_id;
    let active_buffer = tab.active_buffer().id;
    tab.split_active_view(SplitAxis::Vertical).unwrap();
    let other_view = tab
        .open_buffer_as_split(
            BufferState::new("other".to_owned(), "other".to_owned(), None),
            SplitAxis::Vertical,
            false,
            0.5,
        )
        .unwrap();
    assert!(tab.activate_view(active_view));
    seed_layout_caches(&mut tab);
    app.tab_manager = TabManager::for_test_tabs(vec![tab]);

    toggle_active_buffer_reading_order(&mut app);
    assert!(app.tab_manager.session_dirty);
    let tab = app.tab_manager.active_tab().unwrap();
    assert!(tab.active_buffer().right_to_left_reading_order);
    assert!(
        !tab.buffer_for_view(other_view)
            .unwrap()
            .right_to_left_reading_order
    );
    for view in &tab.layout.views {
        assert_eq!(
            view.layout_cache.is_empty(),
            view.buffer_id == active_buffer
        );
    }

    app.tab_manager.clear_session_dirty();
    toggle_active_buffer_control_chars(&mut app);
    assert!(app.tab_manager.session_dirty);
    let tab = app.tab_manager.active_tab().unwrap();
    assert!(tab.active_buffer().show_control_chars);
    assert!(!tab.buffer_for_view(other_view).unwrap().show_control_chars);

    toggle_active_buffer_control_chars(&mut app);
    toggle_active_buffer_reading_order(&mut app);
    let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
    assert!(!buffer.show_control_chars);
    assert!(!buffer.right_to_left_reading_order);
}

fn seed_layout_caches(tab: &mut WorkspaceTab) {
    let ctx = egui::Context::default();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        let galley = ui.painter().layout_no_wrap(
            "sample".to_owned(),
            egui::FontId::default(),
            egui::Color32::WHITE,
        );
        for view in &mut tab.layout.views {
            view.layout_cache.insert(
                LayoutCacheKey {
                    revision: 0,
                    char_range: 0..6,
                    font_family: egui::FontFamily::Proportional,
                    font_size_bits: 14.0_f32.to_bits(),
                    wrap_width_bits: f32::INFINITY.to_bits(),
                    word_wrap: false,
                    show_control_chars: false,
                    right_to_left_reading_order: false,
                    text_color: egui::Color32::WHITE,
                    dark_mode: true,
                    selection_highlight: None,
                    search_highlights: SearchHighlightState::default(),
                    replacement_preview_signature: 0,
                },
                galley.clone(),
                6,
            );
        }
    })
    .drop_without_applying_deltas();
}
