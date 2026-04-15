use super::{SearchScope, ScratchpadApp};
use super::helpers::{cursor_range_from_char_range, preview_for_match};
use crate::app::services::session_store::SessionStore;
use std::thread;
use std::time::{Duration, Instant};

fn test_app() -> ScratchpadApp {
    let session_root = tempfile::tempdir().expect("create session dir");
    let session_store = SessionStore::new(session_root.path().to_path_buf());
    ScratchpadApp::with_session_store(session_store)
}

fn wait_for_search_matches(app: &mut ScratchpadApp, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        app.poll_search();
        if app.search_match_count() == expected {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!(
        "timed out waiting for {expected} search matches; got {}",
        app.search_match_count()
    );
}

#[test]
fn search_result_groups_are_separated_by_tab() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha one".to_owned());
    app.create_untitled_tab();
    app.tabs_mut()[1]
        .buffer
        .replace_text("alpha two".to_owned());
    app.tab_manager_mut().active_tab_index = 0;

    app.open_search();
    app.set_search_scope(SearchScope::AllOpenTabs);
    app.set_search_query("alpha");

    wait_for_search_matches(&mut app, 2);
    let groups = app.search_result_groups();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].entries.len(), 1);
    assert_eq!(groups[1].entries.len(), 1);
}

#[test]
fn activating_search_match_navigates_to_matching_tab_and_range() {
    let mut app = test_app();
    app.tabs_mut()[0].buffer.replace_text("zzz".to_owned());
    app.create_untitled_tab();
    app.tabs_mut()[1]
        .buffer
        .replace_text("alpha target".to_owned());
    app.tab_manager_mut().active_tab_index = 0;

    app.open_search();
    app.set_search_scope(SearchScope::AllOpenTabs);
    app.set_search_query("alpha");
    wait_for_search_matches(&mut app, 1);

    assert!(app.activate_search_match_at(0));
    assert_eq!(app.active_tab_index(), 1);
    let pending = app
        .active_tab()
        .and_then(|tab| tab.active_view())
        .and_then(|view| view.pending_cursor_range);
    assert_eq!(pending, Some(cursor_range_from_char_range(0..5)));
}

#[test]
fn preview_for_match_reports_line_and_column() {
    let (line, column, preview) = preview_for_match("one\ntwo alpha\nthree", &(8..13));
    assert_eq!(line, 2);
    assert_eq!(column, 5);
    assert_eq!(preview, "two alpha");
}