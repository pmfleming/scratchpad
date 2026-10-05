use super::*;
use crate::app::app_state::{ScratchpadApp, settings_controller};
use crate::app::domain::{TabManager, WorkspaceTab};
use crate::app::services::{session_store::SessionStore, settings_store::SettingsStore};
use crate::app::startup::StartupOptions;
use crate::app::ui::tab_drag::{self, TabDropAxis, TabDropZone, TabRectEntry};

fn test_app(root: &std::path::Path) -> ScratchpadApp {
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.join("session")),
        SettingsStore::new(root.join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    app.tab_manager = TabManager::for_test_tabs(vec![
        WorkspaceTab::untitled(),
        WorkspaceTab::untitled(),
        WorkspaceTab::untitled(),
    ]);
    app.tab_manager.set_active_tab_index_clamped(0);
    app
}

fn drop_frame(app: &mut ScratchpadApp, sources: Vec<usize>, pointer: egui::Pos2, down: bool) {
    let ctx = egui::Context::default();
    let start_pos = egui::pos2(120.0, 20.0);
    ctx.data_mut(|data| {
        data.insert_temp(
            tab_drag_state_id(),
            TabDragState {
                source_index: sources[0],
                dragged_indices: sources,
                start_pos,
                current_pos: start_pos,
            },
        );
    });
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        events: vec![
            egui::Event::PointerMoved(pointer),
            egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: down,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        ..Default::default()
    };
    ctx.run_ui(input, |ui| {
        // The strip renders first and must leave unmatched releases available to the editor.
        let zones = [TabDropZone {
            axis: TabDropAxis::Horizontal,
            entries: (0..3)
                .map(|index| TabRectEntry {
                    index,
                    rect: egui::Rect::from_min_size(
                        egui::pos2(index as f32 * 100.0, 0.0),
                        egui::vec2(100.0, 30.0),
                    ),
                    combine_enabled: true,
                })
                .collect(),
        }];
        assert!(tab_drag::update_tab_drag(ui, &zones, 3).is_none());
        assert!(current_tab_drag_state(ui).is_some());
        tab_drag::handle_editor_drop(
            ui,
            app,
            egui::Rect::from_min_max(egui::pos2(0.0, 40.0), egui::pos2(800.0, 550.0)),
        );
        tab_drag::finish_drag_frame(ui);
        assert_eq!(current_tab_drag_state(ui).is_some(), down);
    })
    .drop_without_applying_deltas();
}

#[test]
fn release_over_editor_combines_tabs() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = test_app(directory.path());
    drop_frame(&mut app, vec![1], egui::pos2(400.0, 300.0), false);
    assert_eq!(app.tab_manager.tabs.len(), 2);
    assert_eq!(
        app.tab_manager
            .active_tab()
            .unwrap()
            .layout
            .root_pane
            .leaf_count(),
        2
    );
}

#[test]
fn release_over_editor_combines_selected_group() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = test_app(directory.path());
    drop_frame(&mut app, vec![1, 2], egui::pos2(400.0, 300.0), false);
    assert_eq!(app.tab_manager.tabs.len(), 1);
    assert_eq!(
        app.tab_manager
            .active_tab()
            .unwrap()
            .layout
            .root_pane
            .leaf_count(),
        3
    );
}

#[test]
fn hover_does_not_commit_and_invalid_releases_are_cleared() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = test_app(directory.path());
    for (sources, pointer, down) in [
        (vec![1], egui::pos2(400.0, 300.0), true),
        (vec![0], egui::pos2(400.0, 300.0), false),
        (vec![0, 1], egui::pos2(400.0, 300.0), false),
        (vec![1], egui::pos2(400.0, 580.0), false),
        (vec![1], egui::pos2(120.0, 39.0), false),
    ] {
        drop_frame(&mut app, sources, pointer, down);
        assert_eq!(app.tab_manager.tabs.len(), 3);
        assert_eq!(
            app.tab_manager
                .active_tab()
                .unwrap()
                .layout
                .root_pane
                .leaf_count(),
            1
        );
    }
}

#[test]
fn settings_slot_is_not_an_editor_drop_source() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = test_app(directory.path());
    settings_controller::open_settings_preserving_tab_selection(&mut app);
    app.state.settings_tab_index = 1;
    drop_frame(&mut app, vec![1], egui::pos2(400.0, 300.0), false);
    assert_eq!(app.tab_manager.tabs.len(), 3);
    // Slot 2 now refers to workspace tab 1, not workspace tab 2.
    let source_id = app.tab_manager.tabs[1].buffers.buffer.id;
    drop_frame(&mut app, vec![2], egui::pos2(400.0, 300.0), false);
    assert_eq!(app.tab_manager.tabs.len(), 2);
    assert!(
        app.tab_manager
            .active_tab()
            .unwrap()
            .buffers
            .by_id(source_id)
            .is_some()
    );
}
