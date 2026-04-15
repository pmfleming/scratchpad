#![forbid(unsafe_code)]

use scratchpad::ScratchpadApp;
use scratchpad::app::app_state::SearchScope;
use scratchpad::app::domain::{BufferState, SplitAxis};
use scratchpad::app::services::session_store::SessionStore;
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
fn all_open_tabs_search_counts_matches_across_tabs() {
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
    assert_eq!(app.search_match_count(), 2);
    assert_eq!(app.search_active_match_index(), Some(0));
}

#[test]
fn all_open_tabs_search_matches_decoded_text_from_different_encodings() {
    let mut app = test_app();
    let cafe = "caf\u{00e9}";
    app.tabs_mut()[0].buffer.name = "utf8.txt".to_owned();
    app.tabs_mut()[0]
        .buffer
        .replace_text(format!("{cafe} au lait"));
    app.tabs_mut()[0].buffer.format.encoding_name = "UTF-8".to_owned();
    app.tabs_mut()[0]
        .open_buffer_as_split(
            BufferState::with_encoding(
                "windows1252.txt".to_owned(),
                format!("{cafe} noir"),
                None,
                "windows-1252".to_owned(),
                false,
            ),
            SplitAxis::Vertical,
            false,
            0.5,
        )
        .expect("open split buffer");

    app.open_search();
    app.set_search_scope(SearchScope::AllOpenTabs);
    app.set_search_query(cafe);

    wait_for_search_matches(&mut app, 2);
    assert_eq!(app.search_match_count(), 2);
}

#[test]
fn all_open_tabs_search_counts_matches_across_multiple_buffers() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha one".to_owned());
    app.tabs_mut()[0]
        .open_buffer_as_split(
            BufferState::new("two.txt".to_owned(), "alpha two".to_owned(), None),
            SplitAxis::Vertical,
            false,
            0.5,
        )
        .expect("open split buffer");
    app.create_untitled_tab();
    app.tabs_mut()[1]
        .buffer
        .replace_text("alpha three".to_owned());

    app.open_search();
    app.set_search_scope(SearchScope::AllOpenTabs);
    app.set_search_query("alpha");

    wait_for_search_matches(&mut app, 3);
    assert_eq!(app.search_match_count(), 3);
}

#[test]
fn search_prefers_active_buffer_match_when_scope_is_broader() {
    let mut app = test_app();
    app.tabs_mut()[0].buffer.replace_text("zzz".to_owned());
    app.create_untitled_tab();
    app.tabs_mut()[1]
        .buffer
        .replace_text("alpha one".to_owned());
    app.tab_manager_mut().active_tab_index = 1;

    app.open_search();
    app.set_search_scope(SearchScope::AllOpenTabs);
    app.set_search_query("alpha");

    wait_for_search_matches(&mut app, 1);
    assert_eq!(app.search_active_match_index(), Some(0));
    assert_eq!(app.search_match_count(), 1);
}

#[test]
fn replace_all_only_changes_the_active_buffer() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha beta alpha".to_owned());
    app.create_untitled_tab();
    app.tabs_mut()[1]
        .buffer
        .replace_text("alpha gamma".to_owned());
    app.tab_manager_mut().active_tab_index = 0;

    app.open_search();
    app.set_search_scope(SearchScope::AllOpenTabs);
    app.set_search_query("alpha");
    app.set_search_replacement("omega");

    wait_for_search_matches(&mut app, 3);
    assert!(app.replace_all_search_matches_in_active_buffer());
    assert_eq!(app.tabs()[0].active_buffer().text(), "omega beta omega");
    assert_eq!(app.tabs()[1].active_buffer().text(), "alpha gamma");
}

#[test]
fn toggle_search_opens_then_closes_search_strip() {
    let mut app = test_app();

    assert!(!app.search_open());

    app.toggle_search();
    assert!(app.search_open());

    app.toggle_search();
    assert!(!app.search_open());
}
