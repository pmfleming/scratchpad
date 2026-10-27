use super::{FileController, FileService, PathBuf, ScratchpadApp, default_save_as_file_name};
use crate::app::app_state::test_support::{test_app, wait_for_background_io};
use crate::app::domain::{BufferFreshness, BufferState, PendingAction, TabManager, WorkspaceTab};

mod reload;

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

fn app_with_buffer(root: &std::path::Path, buffer: BufferState) -> ScratchpadApp {
    app_with_tabs(root, vec![WorkspaceTab::new(buffer)])
}

fn app_with_tabs(root: &std::path::Path, tabs: Vec<WorkspaceTab>) -> ScratchpadApp {
    let mut app = test_app(root);
    app.tab_manager = TabManager::for_test_tabs(tabs);
    app
}

fn saved_buffer(name: &str, text: &str, path: PathBuf) -> BufferState {
    let mut buffer = BufferState::new(name.to_owned(), text.to_owned(), Some(path.clone()));
    buffer.sync_to_disk_state(FileService::read_disk_state(&path).ok());
    buffer
}

#[test]
fn saving_an_existing_path_or_confirmed_conflict_clears_only_the_written_dirty_state() {
    for overwrite in [false, true] {
        let mut fixture = SavedBufferFixture::dirty("tracked.txt", "original", "ours");
        let queued = if overwrite {
            fixture.app.tab_manager.tabs.as_mut_slice()[0]
                .active_buffer_mut()
                .mark_conflict_on_disk(FileService::read_disk_state(&fixture.path).ok());
            FileController::save_conflict_overwrite(&mut fixture.app, 0)
        } else {
            FileController::save_file_at(&mut fixture.app, 0)
        };
        assert!(queued, "overwrite={overwrite}");
        assert!(
            fixture.app.tab_manager.tabs.as_slice()[0]
                .active_buffer()
                .is_dirty
        );
        wait_for_background_io(&mut fixture.app);
        assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), "ours");
        let buffer = fixture.app.tab_manager.tabs.as_slice()[0].active_buffer();
        assert!(!buffer.is_dirty);
        assert_eq!(buffer.freshness, BufferFreshness::InSync);
        assert!(buffer.disk_state.is_some());
    }
}

#[test]
fn save_existing_path_stops_when_dirty_buffer_conflicts_with_disk() {
    let mut fixture = SavedBufferFixture::dirty("tracked.txt", "original", "ours");
    let view_id = fixture.app.tab_manager.tabs.as_slice()[0]
        .layout
        .active_view_id;
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
        crate::app::app_state::workspace::accessors::pending_action(&fixture.app),
        Some(PendingAction::SaveConflict {
            tab_index: 0,
            view_id
        })
    );
}

#[test]
fn save_with_encoding_failure_leaves_file_and_buffer_state_unchanged() {
    let mut fixture = SavedBufferFixture::dirty("ansi.txt", "old", "plain 😀");

    assert!(FileController::save_file_with_encoding_at(
        &mut fixture.app,
        0,
        "windows-1252"
    ));
    wait_for_background_io(&mut fixture.app);

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
    wait_for_background_io(&mut app);

    let buffer = app.tab_manager.tabs.as_slice()[0].active_buffer();
    assert_eq!(buffer.path.as_deref(), Some(path.as_path()));
    assert_eq!(buffer.name, "new-name.txt");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "content");
    assert!(buffer.disk_state.is_some());
}

#[test]
fn overlapping_save_as_is_rejected_before_writing_a_second_path() {
    let directory = tempfile::tempdir().unwrap();
    let first_path = directory.path().join("first.txt");
    let second_path = directory.path().join("second.txt");
    let buffer = BufferState::new("Untitled".to_owned(), "content".to_owned(), None);
    let mut app = app_with_buffer(directory.path(), buffer);

    assert!(FileController::save_buffer_to_path(
        &mut app,
        0,
        first_path.clone(),
        true,
        None
    ));
    assert!(!FileController::save_buffer_to_path(
        &mut app,
        0,
        second_path.clone(),
        true,
        None
    ));
    wait_for_background_io(&mut app);

    assert_eq!(
        app.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .path
            .as_deref(),
        Some(first_path.as_path())
    );
    assert_eq!(std::fs::read_to_string(first_path).unwrap(), "content");
    assert!(!second_path.exists());
}

#[test]
fn save_as_to_already_open_path_activates_existing_owner() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.txt");
    std::fs::write(&path, "existing").unwrap();
    let untitled = BufferState::new("Untitled".to_owned(), "new".to_owned(), None);
    let open_owner = saved_buffer("existing.txt", "existing", path.clone());
    let mut app = app_with_tabs(
        directory.path(),
        vec![WorkspaceTab::new(untitled), WorkspaceTab::new(open_owner)],
    );
    let owner_view_id = app.tab_manager.tabs.as_slice()[1].layout.active_view_id();

    assert!(!FileController::save_buffer_to_path(
        &mut app,
        0,
        path.clone(),
        true,
        None
    ));

    assert_eq!(app.tab_manager.active_tab_index, 1);
    assert_eq!(
        app.tab_manager.tabs.as_slice()[1].layout.active_view_id(),
        owner_view_id
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing");
    assert_eq!(
        app.tab_manager.tabs.as_slice()[0].active_buffer().path,
        None
    );
}

#[test]
fn save_completion_keeps_dirty_state_when_buffer_changed_while_write_was_pending() {
    let mut fixture = SavedBufferFixture::dirty("tracked.txt", "old", "saved");

    assert!(FileController::save_file_at(&mut fixture.app, 0));
    assert!(
        crate::app::app_state::workspace::editing::insert_text_in_active_view(
            &mut fixture.app,
            "er"
        )
    );
    wait_for_background_io(&mut fixture.app);

    assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), "saved");
    let buffer = fixture.app.tab_manager.tabs.as_slice()[0].active_buffer();
    assert_eq!(buffer.text(), "saveder");
    assert!(buffer.is_dirty);
    assert_eq!(buffer.freshness, BufferFreshness::InSync);
}

#[test]
fn default_save_as_file_name_adds_txt_only_when_missing_extension() {
    assert_eq!(default_save_as_file_name(""), "untitled.txt");
    assert_eq!(default_save_as_file_name("notes"), "notes.txt");
    assert_eq!(default_save_as_file_name("notes.md"), "notes.md");
}
