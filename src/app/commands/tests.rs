use super::AppCommand;
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{
    BufferFreshness, BufferState, PaneNode, PendingAction, SplitAxis, WorkspaceTab,
};
use crate::app::fonts::EditorFontPreset;
use crate::app::services::file_controller::FileController;
use crate::app::services::session_store::SessionStore;
use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};
use std::fs;

fn collect_leaf_area_fractions(node: &PaneNode, area_fraction: f32, output: &mut Vec<f32>) {
    match node {
        PaneNode::Leaf { .. } => output.push(area_fraction),
        PaneNode::Split {
            ratio,
            first,
            second,
            ..
        } => {
            collect_leaf_area_fractions(first, area_fraction * ratio, output);
            collect_leaf_area_fractions(second, area_fraction * (1.0 - ratio), output);
        }
    }
}

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

#[test]
fn open_text_history_command_opens_history_window() {
    let mut app = test_app();

    app.handle_command(AppCommand::OpenTextHistory);
    assert!(app.text_history_open);

    app.close_text_history();
    assert!(!app.text_history_open);
}

#[test]
fn open_user_manual_opens_a_normal_markdown_file() {
    let temp_dir = tempfile::tempdir().expect("create manual dir");
    let manual_path = temp_dir.path().join("user-manual.md");
    fs::write(
        &manual_path,
        "# Scratchpad\n\nThis is the shipped user manual.\n",
    )
    .expect("write manual");

    let mut app = test_app();
    app.user_manual_path = manual_path.clone();

    app.handle_command(AppCommand::OpenUserManual);
    app.wait_for_background_io_idle();

    assert_eq!(app.tabs().len(), 2);
    let buffer = app.tabs()[app.active_tab_index()].active_buffer();
    assert_eq!(buffer.path.as_deref(), Some(manual_path.as_path()));
    assert_eq!(buffer.name, "user-manual.md");
    assert!(buffer.text().contains("shipped user manual"));
    assert!(!buffer.is_settings_file);

    app.handle_command(AppCommand::OpenUserManual);
    app.wait_for_background_io_idle();

    assert_eq!(app.tabs().len(), 2);
    assert_eq!(
        app.tabs()[app.active_tab_index()]
            .active_buffer()
            .path
            .as_deref(),
        Some(manual_path.as_path())
    );
}

fn settings_file_content(font_size: f32, settings_slot_index: Option<usize>) -> String {
    let mut lines = vec![
        format!("font_size = {font_size:.1}"),
        "word_wrap = false".to_owned(),
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
    app.wait_for_background_io_idle();
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
fn closing_dirty_non_active_view_in_multi_file_tab_requires_confirmation() {
    let mut app = test_app();
    app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
    app.tabs_mut()[0].buffer.replace_text("one".to_owned());

    let second_view_id = app.tabs_mut()[0]
        .open_buffer_as_split(
            BufferState::new("two.txt".to_owned(), "two".to_owned(), None),
            SplitAxis::Vertical,
            false,
            0.5,
        )
        .expect("open second split");
    let third_view_id = app.tabs_mut()[0]
        .open_buffer_as_split(
            BufferState::new("three.txt".to_owned(), "three".to_owned(), None),
            SplitAxis::Horizontal,
            false,
            0.5,
        )
        .expect("open third split");

    app.handle_command(AppCommand::ActivateView {
        view_id: third_view_id,
    });

    let second_buffer_id = app.tabs()[0]
        .view(second_view_id)
        .expect("second view should exist")
        .buffer_id;
    let second_buffer = app.tabs_mut()[0]
        .buffer_by_id_mut(second_buffer_id)
        .expect("second buffer should exist");
    second_buffer.is_dirty = true;
    second_buffer.mark_conflict_on_disk(None);

    app.handle_command(AppCommand::CloseView {
        view_id: second_view_id,
    });

    assert!(matches!(
        app.pending_action(),
        Some(PendingAction::CloseView { tab_index: 0, view_id }) if view_id == second_view_id
    ));
    assert_eq!(app.tabs()[0].active_view_id, third_view_id);
    assert!(app.tabs()[0].view(second_view_id).is_some());
}

#[test]
fn closing_dirty_duplicate_view_only_prompts_for_last_remaining_view() {
    let mut app = test_app();
    app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
    app.tabs_mut()[0].buffer.replace_text("one".to_owned());

    let duplicate_view_id = app.tabs_mut()[0]
        .split_active_view(SplitAxis::Vertical)
        .expect("split duplicate view");
    let other_file_view_id = app.tabs_mut()[0]
        .open_buffer_as_split(
            BufferState::new("two.txt".to_owned(), "two".to_owned(), None),
            SplitAxis::Horizontal,
            false,
            0.5,
        )
        .expect("open second file");
    let original_view_id = app.tabs()[0]
        .views
        .iter()
        .find(|view| view.id != duplicate_view_id && view.buffer_id == app.tabs()[0].buffer.id)
        .expect("original view should remain")
        .id;

    app.tabs_mut()[0].buffer.is_dirty = true;
    app.handle_command(AppCommand::ActivateView {
        view_id: other_file_view_id,
    });

    app.handle_command(AppCommand::CloseView {
        view_id: duplicate_view_id,
    });

    assert_eq!(app.pending_action(), None);
    assert!(app.tabs()[0].view(duplicate_view_id).is_none());
    assert!(app.tabs()[0].view(original_view_id).is_some());

    app.handle_command(AppCommand::CloseView {
        view_id: original_view_id,
    });

    assert!(matches!(
        app.pending_action(),
        Some(PendingAction::CloseView { tab_index: 0, view_id }) if view_id == original_view_id
    ));
    assert!(app.tabs()[0].view(original_view_id).is_some());
}

#[test]
fn closing_view_prunes_removed_buffer_text_history_entries() {
    let mut app = test_app();
    app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
    let second_view_id = app.tabs_mut()[0]
        .open_buffer_as_split(
            BufferState::new("two.txt".to_owned(), "alpha".to_owned(), None),
            SplitAxis::Vertical,
            false,
            0.5,
        )
        .expect("open second file");
    let second_buffer_id = app.tabs()[0]
        .view(second_view_id)
        .expect("second view should exist")
        .buffer_id;
    let selection = CursorRange::one(CharCursor::new(0));

    app.tabs_mut()[0]
        .buffer_by_id_mut(second_buffer_id)
        .expect("second buffer should exist")
        .replace_char_ranges_with_undo(&[(0..5, "omega".to_owned())], selection, selection)
        .expect("replace second buffer text");
    app.record_pending_text_history_event(0, second_buffer_id);
    assert_eq!(app.text_history_len_for_buffer(second_buffer_id), 1);

    app.perform_close_view(second_view_id);

    assert_eq!(app.text_history_len_for_buffer(second_buffer_id), 0);
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
    app.wait_for_background_io_idle();

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
    app.wait_for_background_io_idle();
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
    app.wait_for_background_io_idle();
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

#[test]
fn activating_clean_tab_reloads_newer_disk_content() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let first_path = temp_dir.path().join("first.txt");
    let second_path = temp_dir.path().join("second.txt");
    fs::write(&first_path, "alpha\n").expect("write first temp file");
    fs::write(&second_path, "beta\n").expect("write second temp file");

    let mut app = test_app();
    FileController::open_external_paths(&mut app, vec![first_path.clone(), second_path]);
    let (first_tab_index, _) = app
        .find_tab_by_path(&first_path)
        .expect("first file should be open");

    fs::write(&first_path, "alpha changed on disk\n").expect("overwrite first temp file");
    app.handle_command(AppCommand::ActivateTab {
        index: first_tab_index,
    });
    app.wait_for_background_io_idle();

    let buffer = app.tabs()[app.active_tab_index()].active_buffer();
    assert_eq!(buffer.text(), "alpha changed on disk\n");
    assert_eq!(buffer.freshness, BufferFreshness::InSync);
}

#[test]
fn startup_restore_conflict_can_open_disk_version_for_comparison() {
    let session_root = tempfile::tempdir().expect("create session root");
    let session_path = session_root.path().to_path_buf();
    let file_path = session_root.path().join("notes.txt");
    fs::write(&file_path, "disk original\n").expect("write original disk version");

    let mut original = ScratchpadApp::with_session_store(SessionStore::new(session_path.clone()));
    FileController::open_external_paths(&mut original, vec![file_path.clone()]);
    let file_tab_index = original.active_tab_index();
    let buffer = original.tabs_mut()[file_tab_index].active_buffer_mut();
    buffer.replace_text("session version\n".to_owned());
    buffer.is_dirty = true;
    original.persist_session_now().expect("persist session");
    drop(original);

    fs::write(&file_path, "disk changed\n").expect("write changed disk version");

    let mut restored = ScratchpadApp::with_session_store(SessionStore::new(session_path));
    let (restored_file_tab_index, _) = restored
        .find_tab_by_path(&file_path)
        .expect("restored file tab should exist");
    assert_eq!(
        restored.tabs()[restored_file_tab_index]
            .active_buffer()
            .text(),
        "session version\n"
    );
    assert_eq!(
        restored.tabs()[restored_file_tab_index]
            .active_buffer()
            .freshness,
        BufferFreshness::ConflictOnDisk
    );
    assert_eq!(restored.startup_restore_conflict_count(), 1);

    assert!(restored.open_disk_version_for_current_startup_restore_conflict());
    restored.wait_for_background_io_idle();
    assert_eq!(restored.startup_restore_conflict_count(), 0);
    let compare_tab_index = restored.active_tab_index();
    assert_eq!(restored.tabs().len(), 3);
    assert_eq!(
        restored.tabs()[restored_file_tab_index]
            .active_buffer()
            .text(),
        "session version\n"
    );
    assert_eq!(
        restored.tabs()[compare_tab_index].active_buffer().text(),
        "disk changed\n"
    );
    assert_eq!(
        restored.tabs()[compare_tab_index].active_buffer().name,
        "notes.txt (Disk)"
    );
    assert!(
        restored.tabs()[compare_tab_index]
            .active_buffer()
            .path
            .is_none()
    );
}

#[test]
fn combining_multiple_tabs_rebalances_workspace_equally() {
    let mut app = app_with_named_tabs(&["one.txt", "two.txt", "three.txt", "four.txt"]);

    app.handle_command(AppCommand::CombineTabsIntoTab {
        source_indices: vec![1, 2, 3],
        target_index: 0,
    });

    assert_eq!(app.tabs().len(), 1);

    let mut areas = Vec::new();
    collect_leaf_area_fractions(&app.tabs()[0].root_pane, 1.0, &mut areas);

    assert_eq!(areas.len(), 4);
    assert!(areas.iter().all(|area| (area - 0.25).abs() < f32::EPSILON));
}

#[test]
fn combining_single_tab_rebalances_workspace_equally() {
    let mut app = app_with_named_tabs(&["one.txt", "two.txt"]);

    app.handle_command(AppCommand::CombineTabIntoTab {
        source_index: 1,
        target_index: 0,
    });

    assert_eq!(app.tabs().len(), 1);

    let mut areas = Vec::new();
    collect_leaf_area_fractions(&app.tabs()[0].root_pane, 1.0, &mut areas);

    assert_eq!(areas.len(), 2);
    assert!(areas.iter().all(|area| (area - 0.5).abs() < f32::EPSILON));
}
