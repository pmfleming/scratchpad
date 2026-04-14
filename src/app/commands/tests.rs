use super::AppCommand;
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{BufferState, SplitAxis, WorkspaceTab};
use crate::app::fonts::EditorFontPreset;
use crate::app::services::session_store::SessionStore;

fn test_app() -> ScratchpadApp {
    let session_root = tempfile::tempdir().expect("create session dir");
    let session_store = SessionStore::new(session_root.path().to_path_buf());
    ScratchpadApp::with_session_store(session_store)
}

fn app_with_named_tabs(names: &[&str]) -> ScratchpadApp {
    let mut app = test_app();
    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            app.append_tab(WorkspaceTab::untitled());
        }
        app.tabs_mut()[index].buffer.name = (*name).to_owned();
    }
    app
}

fn settings_file_content(font_size: f32, settings_slot_index: Option<usize>) -> String {
    let mut lines = vec![
        format!("font_size = {font_size:.1}"),
        "word_wrap = false".to_owned(),
        "logging_enabled = false".to_owned(),
        "editor_font = \"standard\"".to_owned(),
    ];
    if let Some(index) = settings_slot_index {
        lines.push("settings_tab_open = true".to_owned());
        lines.push(format!("settings_tab_index = {index}"));
    } else {
        lines.push("settings_tab_open = false".to_owned());
    }
    lines.push(String::new());
    lines.join("\n")
}

fn open_dirty_settings_file(
    app: &mut ScratchpadApp,
    font_size: f32,
    settings_slot_index: Option<usize>,
) -> usize {
    app.handle_command(AppCommand::OpenSettings);
    app.open_settings_file_tab();
    let settings_tab_index = app.active_tab_index();
    app.tabs_mut()[settings_tab_index]
        .active_buffer_mut()
        .replace_text(settings_file_content(font_size, settings_slot_index));
    app.tabs_mut()[settings_tab_index]
        .active_buffer_mut()
        .is_dirty = true;
    app.note_settings_toml_edit(settings_tab_index);
    settings_tab_index
}

fn assert_settings_applied(app: &ScratchpadApp, font_size: f32) {
    assert_eq!(app.font_size(), font_size);
    assert!(!app.word_wrap());
    assert!(!app.logging_enabled());
    assert_eq!(app.editor_font(), EditorFontPreset::Standard);
}

#[test]
fn promote_view_to_tab_creates_a_new_active_tab() {
    let mut app = test_app();
    app.tabs_mut()[0].buffer.name = "alpha.txt".to_owned();
    app.tabs_mut()[0].buffer.replace_text("alpha".to_owned());
    let promoted_view_id = app.tabs_mut()[0]
        .split_active_view(SplitAxis::Vertical)
        .expect("split should succeed");
    let first_view_id = app.tabs()[0].views[0].id;
    app.tabs_mut()[0].activate_view(first_view_id);
    app.tabs_mut()[0]
        .open_buffer_as_split(
            BufferState::new("beta.txt".to_owned(), "beta".to_owned(), None),
            SplitAxis::Horizontal,
            false,
            0.5,
        )
        .expect("open buffer split should succeed");

    app.handle_command(AppCommand::PromoteViewToTab {
        view_id: promoted_view_id,
    });

    assert_eq!(app.tabs().len(), 2);
    assert_eq!(app.active_tab_index(), 1);
    assert_eq!(app.tabs()[1].views.len(), 2);
    assert_eq!(app.tabs()[1].active_view_id, promoted_view_id);
    assert_eq!(app.tabs()[1].active_buffer().name, "alpha.txt");
    assert_eq!(app.tabs()[0].views.len(), 1);
    assert_eq!(app.tabs()[0].active_buffer().name, "beta.txt");
    assert_eq!(app.pending_editor_focus, Some(promoted_view_id));
}

#[test]
fn promote_tab_files_to_tabs_splits_workspace_into_individual_tabs() {
    let mut app = test_app();
    app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
    app.tabs_mut()[0].buffer.replace_text("one".to_owned());

    for (name, content) in [("two.txt", "two"), ("three.txt", "three")] {
        app.tabs_mut()[0]
            .open_buffer_as_split(
                BufferState::new(name.to_owned(), content.to_owned(), None),
                SplitAxis::Vertical,
                false,
                0.5,
            )
            .expect("open buffer split should succeed");
    }

    assert!(app.tabs()[0].can_promote_all_files());
    let active_name = app.tabs()[0].active_buffer().name.to_owned();

    app.handle_command(AppCommand::PromoteTabFilesToTabs { index: 0 });

    assert_eq!(app.tabs().len(), 3);
    assert!(app.tabs().iter().all(|tab| tab.file_group_count() == 1));
    assert_eq!(
        app.tabs()[app.active_tab_index()].active_buffer().name,
        active_name
    );
    assert_eq!(
        app.pending_editor_focus,
        Some(app.tabs()[app.active_tab_index()].active_view_id)
    );
}

#[test]
fn activating_a_tab_queues_focus_for_its_active_view() {
    let mut app = test_app();
    app.append_tab(WorkspaceTab::untitled());
    app.pending_editor_focus = None;

    let first_tab_view_id = app.tabs()[0].active_view_id;
    app.handle_command(AppCommand::ActivateTab { index: 0 });

    assert_eq!(app.active_tab_index(), 0);
    assert_eq!(app.pending_editor_focus, Some(first_tab_view_id));
}

#[test]
fn activating_a_view_queues_focus_for_that_view() {
    let mut app = test_app();
    let second_view_id = app.tabs_mut()[0]
        .split_active_view(SplitAxis::Vertical)
        .expect("split should succeed");
    app.pending_editor_focus = None;

    app.handle_command(AppCommand::ActivateView {
        view_id: second_view_id,
    });

    assert_eq!(app.tabs()[0].active_view_id, second_view_id);
    assert_eq!(app.pending_editor_focus, Some(second_view_id));
}

#[test]
fn settings_commands_switch_between_workspace_and_settings_surface() {
    let mut app = test_app();

    app.handle_command(AppCommand::OpenSettings);
    assert!(app.showing_settings());
    assert_eq!(app.settings_slot_index(), Some(app.tabs().len()));

    app.handle_command(AppCommand::CloseSettings);
    assert!(!app.showing_settings());
    assert_eq!(app.settings_slot_index(), None);
    assert_eq!(
        app.pending_editor_focus,
        Some(app.tabs()[app.active_tab_index()].active_view_id)
    );
}

#[test]
fn reorder_display_tab_moves_settings_slot_between_workspace_tabs() {
    let mut app = app_with_named_tabs(&["one.txt", "two.txt", "three.txt"]);

    app.handle_command(AppCommand::OpenSettings);
    let settings_slot = app
        .settings_slot_index()
        .expect("settings slot should exist");
    assert_eq!(settings_slot, 3);

    app.handle_command(AppCommand::ReorderDisplayTab {
        from_index: settings_slot,
        to_index: 1,
    });

    assert_eq!(app.settings_slot_index(), Some(1));
    assert_eq!(app.workspace_index_for_slot(0), Some(0));
    assert_eq!(app.workspace_index_for_slot(2), Some(1));
    assert_eq!(app.workspace_index_for_slot(3), Some(2));
    assert_eq!(app.display_tab_name_at_slot(1).as_deref(), Some("Settings"));
}

#[test]
fn settings_surface_state_persists_across_restart() {
    let session_root = tempfile::tempdir().expect("create session dir");
    let original_path = session_root.path().to_path_buf();
    let restored_path = session_root.path().to_path_buf();

    let mut original = ScratchpadApp::with_session_store(SessionStore::new(original_path));
    for (index, name) in ["one.txt", "two.txt", "three.txt"].iter().enumerate() {
        if index > 0 {
            original.append_tab(WorkspaceTab::untitled());
        }
        original.tabs_mut()[index].buffer.name = (*name).to_owned();
    }

    original.handle_command(AppCommand::OpenSettings);
    original.handle_command(AppCommand::ReorderDisplayTab {
        from_index: 3,
        to_index: 1,
    });

    let persisted = original
        .settings_store
        .load()
        .expect("load persisted settings")
        .expect("settings should exist");
    assert!(persisted.settings_tab_open);
    assert_eq!(persisted.settings_tab_index, Some(1));

    drop(original);

    let restored = ScratchpadApp::with_session_store(SessionStore::new(restored_path));
    assert!(restored.showing_settings());
    assert_eq!(restored.settings_slot_index(), Some(1));
    assert_eq!(
        restored.display_tab_name_at_slot(1).as_deref(),
        Some("Settings")
    );
}

#[test]
fn activating_a_workspace_tab_keeps_the_settings_tab_open() {
    let mut app = app_with_named_tabs(&["one.txt", "two.txt"]);

    app.handle_command(AppCommand::OpenSettings);
    let settings_slot = app
        .settings_slot_index()
        .expect("settings slot should exist after open");

    app.handle_command(AppCommand::ActivateTab { index: 1 });

    assert!(!app.showing_settings());
    assert_eq!(app.settings_slot_index(), Some(settings_slot));
    assert_eq!(app.active_tab_index(), 1);
    assert_eq!(
        app.display_tab_name_at_slot(settings_slot).as_deref(),
        Some("Settings")
    );
}

#[test]
fn opening_settings_file_keeps_settings_tab_open() {
    let mut app = test_app();
    app.handle_command(AppCommand::OpenSettings);
    let settings_slot = app
        .settings_slot_index()
        .expect("settings slot should exist after open");
    let settings_path = app.settings_path().to_path_buf();

    app.open_settings_file_tab();

    assert!(!app.showing_settings());
    assert_eq!(app.settings_slot_index(), Some(settings_slot));
    assert_eq!(
        app.tabs()[app.active_tab_index()]
            .active_buffer()
            .path
            .as_deref(),
        Some(settings_path.as_path())
    );
    assert!(
        app.tabs()[app.active_tab_index()]
            .active_buffer()
            .is_settings_file
    );
}

#[test]
fn creating_a_new_tab_from_dirty_settings_file_applies_toml_edits() {
    let mut app = test_app();
    open_dirty_settings_file(&mut app, 19.0, Some(1));

    app.new_tab();

    assert_settings_applied(&app, 19.0);
    assert_eq!(
        app.tabs()[app.active_tab_index()].active_buffer().name,
        "Untitled"
    );
}

#[test]
fn activating_away_from_settings_file_applies_toml_edits() {
    let mut app = test_app();
    open_dirty_settings_file(&mut app, 22.0, Some(1));

    app.handle_command(AppCommand::ActivateTab { index: 0 });

    assert_settings_applied(&app, 22.0);
    assert_eq!(app.settings_slot_index(), Some(1));
    assert_eq!(app.active_tab_index(), 0);
    assert!(!app.showing_settings());
}

#[test]
fn activating_view_away_from_settings_file_applies_toml_edits() {
    let mut app = test_app();
    app.handle_command(AppCommand::OpenSettings);
    app.open_settings_file_tab();
    let settings_tab_index = app.active_tab_index();
    let settings_view_id = app.tabs()[settings_tab_index].active_view_id;
    let normal_view_id = app.tabs_mut()[settings_tab_index]
        .open_buffer_as_split(
            BufferState::new("notes.txt".to_owned(), "notes".to_owned(), None),
            SplitAxis::Vertical,
            false,
            0.5,
        )
        .expect("open normal split");

    app.handle_command(AppCommand::ActivateView {
        view_id: settings_view_id,
    });
    let settings_tab_index = app.active_tab_index();
    app.tabs_mut()[settings_tab_index]
        .active_buffer_mut()
        .replace_text(settings_file_content(23.0, None));
    app.tabs_mut()[settings_tab_index]
        .active_buffer_mut()
        .is_dirty = true;
    app.note_settings_toml_edit(settings_tab_index);

    app.handle_command(AppCommand::ActivateView {
        view_id: normal_view_id,
    });

    assert_settings_applied(&app, 23.0);
    assert_eq!(
        app.tabs()[app.active_tab_index()].active_view_id,
        normal_view_id
    );
}

#[test]
fn editing_other_buffer_in_settings_workspace_does_not_mark_settings_toml_pending() {
    let mut app = test_app();
    app.handle_command(AppCommand::OpenSettings);
    app.open_settings_file_tab();
    let settings_path = app.settings_path().to_path_buf();
    let settings_tab_index = app.active_tab_index();
    app.tabs_mut()[settings_tab_index]
        .open_buffer_as_split(
            BufferState::new("notes.txt".to_owned(), "notes".to_owned(), None),
            SplitAxis::Vertical,
            false,
            0.5,
        )
        .expect("open normal split");
    let settings_tab_index = app.active_tab_index();

    let settings_buffer = app.tabs_mut()[settings_tab_index]
        .extra_buffers
        .iter_mut()
        .find(|buffer| {
            buffer
                .path
                .as_ref()
                .is_some_and(|path| crate::app::paths_match(path, &settings_path))
        })
        .expect("settings buffer should remain in the workspace");
    settings_buffer.replace_text(
        [
            "font_size = 31.0",
            "word_wrap = false",
            "logging_enabled = false",
            "editor_font = \"standard\"",
            "",
        ]
        .join("\n"),
    );

    app.tabs_mut()[settings_tab_index]
        .active_buffer_mut()
        .replace_text("changed notes".to_owned());
    app.tabs_mut()[settings_tab_index]
        .active_buffer_mut()
        .is_dirty = true;
    app.note_settings_toml_edit(settings_tab_index);
    app.append_tab(WorkspaceTab::untitled());
    let other_tab_index = app.active_tab_index();

    app.handle_command(AppCommand::ActivateTab {
        index: other_tab_index,
    });

    assert_eq!(app.font_size(), 14.0);
    assert!(app.word_wrap());
    assert!(app.logging_enabled());
}

#[test]
fn closing_saved_settings_file_applies_pending_toml_edits() {
    let mut app = test_app();
    let settings_tab_index = open_dirty_settings_file(&mut app, 20.0, Some(0));

    assert!(app.save_file_at(settings_tab_index));

    app.handle_command(AppCommand::CloseTab {
        index: settings_tab_index,
    });

    assert_settings_applied(&app, 20.0);
    assert!(app.showing_settings());
    assert_eq!(app.settings_slot_index(), Some(0));
}

#[test]
fn closing_dirty_settings_file_applies_buffered_toml_edits() {
    let mut app = test_app();
    let settings_tab_index = open_dirty_settings_file(&mut app, 26.0, Some(0));

    app.handle_command(AppCommand::CloseTab {
        index: settings_tab_index,
    });

    assert_settings_applied(&app, 26.0);
    assert!(app.showing_settings());
    assert_eq!(app.settings_slot_index(), Some(0));
}
