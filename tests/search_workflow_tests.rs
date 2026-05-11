use scratchpad::ScratchpadApp;
use scratchpad::app::app_state::SearchScope;
use scratchpad::app::domain::{BufferState, WorkspaceTab};
use scratchpad::app::services::session_store::SessionStore;
use scratchpad::app::services::settings_store::SettingsStore;
use scratchpad::app::startup::StartupOptions;
use std::time::{Duration, Instant};

fn test_app() -> ScratchpadApp {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.keep();
    ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.clone()),
        SettingsStore::new(root),
        StartupOptions::default(),
    )
}

fn wait_for_search(app: &mut ScratchpadApp) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        app.poll_search();
        if app.search_match_count() > 0 || !app.search_open() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn active_buffer_search_counts_matches() {
    let mut app = test_app();
    app.tabs_mut()[0] = WorkspaceTab::new(BufferState::new(
        "search.txt".to_owned(),
        "foo bar foo".to_owned(),
        None,
    ));

    app.open_search();
    app.set_search_query("foo");
    wait_for_search(&mut app);

    assert_eq!(app.search_match_count(), 2);
    assert_eq!(app.search_active_match_index(), Some(0));
}

#[test]
fn all_open_tabs_search_counts_matches_across_tabs() {
    let mut app = test_app();
    app.tabs_mut()[0] = WorkspaceTab::new(BufferState::new(
        "one.txt".to_owned(),
        "needle one".to_owned(),
        None,
    ));
    app.append_tab(WorkspaceTab::new(BufferState::new(
        "two.txt".to_owned(),
        "needle two needle".to_owned(),
        None,
    )));

    app.open_search();
    app.set_search_scope(SearchScope::AllOpenTabs);
    app.set_search_query("needle");
    wait_for_search(&mut app);

    assert_eq!(app.search_match_count(), 3);
}

#[test]
fn replace_all_changes_every_buffer_in_scope() {
    let mut app = test_app();
    app.tabs_mut()[0] = WorkspaceTab::new(BufferState::new(
        "one.txt".to_owned(),
        "foo one".to_owned(),
        None,
    ));
    app.append_tab(WorkspaceTab::new(BufferState::new(
        "two.txt".to_owned(),
        "foo two foo".to_owned(),
        None,
    )));

    app.open_search_and_replace();
    app.set_search_scope(SearchScope::AllOpenTabs);
    app.set_search_query("foo");
    app.set_search_replacement("bar");
    wait_for_search(&mut app);

    assert!(!app.replace_all_search_matches());
    assert!(app.replace_all_search_matches());
    wait_for_search(&mut app);

    assert_eq!(app.tabs()[0].buffer.text(), "bar one");
    assert_eq!(app.tabs()[1].buffer.text(), "bar two bar");
}

#[test]
fn toggle_search_opens_then_closes_search_strip() {
    let mut app = test_app();

    app.toggle_search();
    assert!(app.search_open());

    app.toggle_search();
    assert!(!app.search_open());
}
