use super::{FileController, FileService, PathBuf, ScratchpadApp, default_save_as_file_name};
use crate::app::app_state::{PendingReloadBufferAction, PendingReloadMode};
use crate::app::domain::{BufferFreshness, BufferState, PendingAction, TabManager, WorkspaceTab};
use crate::app::services::background_io::LoadedPathResult;
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::SettingsStore;
use crate::app::startup::StartupOptions;

struct SavedBufferFixture {
    _directory: tempfile::TempDir,
    path: PathBuf,
    app: ScratchpadApp,
}

impl SavedBufferFixture {
    fn new(file_name: &str, disk_text: &str, buffer_text: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(file_name);
        std::fs::write(&path, disk_text).unwrap();
        let app = app_with_buffer(
            directory.path(),
            saved_buffer(file_name, buffer_text, path.clone()),
        );

        Self {
            _directory: directory,
            path,
            app,
        }
    }

    fn dirty(file_name: &str, disk_text: &str, buffer_text: &str) -> Self {
        let mut fixture = Self::new(file_name, disk_text, buffer_text);
        fixture.app.tab_manager.tabs.as_mut_slice()[0]
            .active_buffer_mut()
            .is_dirty = true;
        fixture
    }
}

fn test_app(root: &std::path::Path) -> ScratchpadApp {
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.join("session")),
        SettingsStore::new(root.join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    app
}

fn app_with_buffer(root: &std::path::Path, buffer: BufferState) -> ScratchpadApp {
    let mut app = test_app(root);
    app.tab_manager = TabManager {
        tabs: vec![WorkspaceTab::new(buffer)],
        active_tab_index: 0,
        pending_action: None,
        session_dirty: false,
        pending_scroll_to_active: false,
        buffer_tab_index: Default::default(),
        cold_session_tabs: Default::default(),
    };
    app.tab_manager.rebuild_buffer_tab_index();
    app
}

fn saved_buffer(name: &str, text: &str, path: PathBuf) -> BufferState {
    let mut buffer = BufferState::new(name.to_owned(), text.to_owned(), Some(path.clone()));
    buffer.sync_to_disk_state(FileService::read_disk_state(&path).ok());
    buffer
}

#[test]
fn save_existing_path_writes_snapshot_and_clears_dirty_state() {
    let mut fixture = SavedBufferFixture::dirty("tracked.txt", "old", "new");

    assert!(FileController::save_file_at(&mut fixture.app, 0));
    assert!(
        fixture.app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .is_dirty
    );
    fixture.app.wait_for_background_io_idle();

    assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), "new");
    let buffer = fixture.app.tab_manager.tabs.as_slice()[0].active_buffer();
    assert!(!buffer.is_dirty);
    assert_eq!(buffer.freshness, BufferFreshness::InSync);
    assert!(buffer.disk_state.is_some());
}

#[test]
fn save_existing_path_stops_when_dirty_buffer_conflicts_with_disk() {
    let mut fixture = SavedBufferFixture::dirty("tracked.txt", "original", "ours");
    let view_id = fixture.app.tab_manager.tabs.as_slice()[0].active_view_id;
    std::fs::write(&fixture.path, "theirs").unwrap();

    assert!(!FileController::save_file_at(&mut fixture.app, 0));

    assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), "theirs");
    assert_eq!(
        fixture.app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .freshness,
        BufferFreshness::ConflictOnDisk
    );
    assert_eq!(
        fixture.app.pending_action(),
        Some(PendingAction::SaveConflict {
            tab_index: 0,
            view_id
        })
    );
}

#[test]
fn save_conflict_overwrite_writes_buffer_text_after_confirmation() {
    let mut fixture = SavedBufferFixture::dirty("tracked.txt", "original", "ours");
    fixture.app.tab_manager.tabs.as_mut_slice()[0]
        .active_buffer_mut()
        .mark_conflict_on_disk(FileService::read_disk_state(&fixture.path).ok());

    assert!(FileController::save_conflict_overwrite(&mut fixture.app, 0));
    fixture.app.wait_for_background_io_idle();

    assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), "ours");
    let buffer = fixture.app.tab_manager.tabs.as_slice()[0].active_buffer();
    assert!(!buffer.is_dirty);
    assert_eq!(buffer.freshness, BufferFreshness::InSync);
}

#[test]
fn save_with_encoding_failure_leaves_file_and_buffer_state_unchanged() {
    let mut fixture = SavedBufferFixture::dirty("ansi.txt", "old", "plain 😀");

    assert!(FileController::save_file_with_encoding_at(
        &mut fixture.app,
        0,
        "windows-1252"
    ));
    fixture.app.wait_for_background_io_idle();

    assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), "old");
    assert!(
        fixture.app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .is_dirty
    );
    assert_eq!(
        fixture.app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .format
            .encoding_name,
        "UTF-8"
    );
}

#[test]
fn save_as_path_assignment_uses_written_disk_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new-name.txt");
    let buffer = BufferState::new("Untitled".to_owned(), "content".to_owned(), None);
    let mut app = app_with_buffer(directory.path(), buffer);

    assert!(FileController::save_buffer_to_path(
        &mut app,
        0,
        path.clone(),
        true,
        None
    ));
    app.wait_for_background_io_idle();

    let buffer = app.tab_manager.tabs.as_slice()[0].active_buffer();
    assert_eq!(buffer.path.as_deref(), Some(path.as_path()));
    assert_eq!(buffer.name, "new-name.txt");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "content");
    assert!(buffer.disk_state.is_some());
}

#[test]
fn save_completion_keeps_dirty_state_when_buffer_changed_while_write_was_pending() {
    let mut fixture = SavedBufferFixture::dirty("tracked.txt", "old", "saved");

    assert!(FileController::save_file_at(&mut fixture.app, 0));
    assert!(fixture.app.insert_text_in_active_view("er"));
    fixture.app.wait_for_background_io_idle();

    assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), "saved");
    let buffer = fixture.app.tab_manager.tabs.as_slice()[0].active_buffer();
    assert_eq!(buffer.text(), "saveder");
    assert!(buffer.is_dirty);
    assert_eq!(buffer.freshness, BufferFreshness::InSync);
}

#[test]
fn explicit_reload_replaces_buffer_from_loaded_disk_result() {
    let mut fixture = SavedBufferFixture::new("tracked.txt", "disk", "memory");
    let buffer_id = fixture.app.tab_manager.tabs.as_slice()[0]
        .active_buffer()
        .id;
    let disk_state = FileService::read_disk_state(&fixture.path).ok();
    let loaded = BufferState::new(
        "tracked.txt".to_owned(),
        "disk".to_owned(),
        Some(fixture.path.clone()),
    );

    FileController::apply_async_reload_buffer_result(
        &mut fixture.app,
        PendingReloadBufferAction {
            buffer_id,
            expected_path: fixture.path.clone(),
            buffer_name: "tracked.txt".to_owned(),
            previous_disk_state: disk_state.clone(),
            mode: PendingReloadMode::ExplicitReload,
        },
        vec![LoadedPathResult {
            path: fixture.path,
            disk_state,
            result: Ok(loaded),
        }],
    );

    assert_eq!(
        fixture.app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .text(),
        "disk"
    );
    assert!(
        !fixture.app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .is_dirty
    );
}

#[test]
fn default_save_as_file_name_adds_txt_only_when_missing_extension() {
    assert_eq!(default_save_as_file_name(""), "untitled.txt");
    assert_eq!(default_save_as_file_name("notes"), "notes.txt");
    assert_eq!(default_save_as_file_name("notes.md"), "notes.md");
}
