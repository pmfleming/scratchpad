#![forbid(unsafe_code)]

use scratchpad::ScratchpadApp;
use scratchpad::app::app_state::SearchScope;
use scratchpad::app::domain::{BufferState, SplitAxis};
use scratchpad::app::services::search::{
    SearchError, SearchMode, SearchOptions, find_matches, next_match_index, previous_match_index,
    search_text, search_text_interruptible,
};
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
fn find_matches_returns_character_ranges() {
    let matches = find_matches("naive cafe", "cafe", SearchOptions::default());
    assert_eq!(matches, vec![6..10]);
}

#[test]
fn find_matches_supports_case_insensitive_search() {
    let matches = find_matches(
        "Alpha alpha ALPHA",
        "alpha",
        SearchOptions {
            mode: SearchMode::PlainText,
            match_case: false,
            whole_word: false,
        },
    );
    assert_eq!(matches, vec![0..5, 6..11, 12..17]);
}

#[test]
fn whole_word_matching_rejects_embedded_hits() {
    let matches = find_matches(
        "cat concatenate cat",
        "cat",
        SearchOptions {
            mode: SearchMode::PlainText,
            match_case: true,
            whole_word: true,
        },
    );
    assert_eq!(matches, vec![0..3, 16..19]);
}

#[test]
fn unicode_search_uses_character_offsets() {
    let matches = find_matches(
        "cafe cafe caf\u{00e9}",
        "caf\u{00e9}",
        SearchOptions::default(),
    );
    assert_eq!(matches, vec![10..14]);
}

#[test]
fn regex_search_supports_case_insensitive_matches() {
    let outcome = search_text(
        "Alpha beta alpha",
        "alpha|beta",
        SearchOptions {
            mode: SearchMode::Regex,
            match_case: false,
            whole_word: false,
        },
    );
    assert_eq!(outcome.matches, vec![0..5, 6..10, 11..16]);
    assert_eq!(outcome.error, None);
}

#[test]
fn regex_search_reports_invalid_queries() {
    let outcome = search_text(
        "Alpha",
        "(",
        SearchOptions {
            mode: SearchMode::Regex,
            match_case: true,
            whole_word: false,
        },
    );
    assert!(outcome.matches.is_empty());
    assert!(matches!(outcome.error, Some(SearchError::InvalidRegex(_))));
}

#[test]
fn regex_search_reports_unbounded_queries_as_unsupported() {
    let outcome = search_text(
        "Alpha beta alpha",
        "alpha+",
        SearchOptions {
            mode: SearchMode::Regex,
            match_case: true,
            whole_word: false,
        },
    );

    assert!(outcome.matches.is_empty());
    assert!(matches!(
        outcome.error,
        Some(SearchError::UnsupportedRegex(_))
    ));
}

#[test]
fn regex_whole_word_uses_character_offsets() {
    let outcome = search_text(
        "cat concatenate cat",
        "cat",
        SearchOptions {
            mode: SearchMode::Regex,
            match_case: true,
            whole_word: true,
        },
    );
    assert_eq!(outcome.matches, vec![0..3, 16..19]);
}

#[test]
fn next_and_previous_match_indices_wrap() {
    assert_eq!(next_match_index(3, None), Some(0));
    assert_eq!(next_match_index(3, Some(2)), Some(0));
    assert_eq!(previous_match_index(3, None), Some(2));
    assert_eq!(previous_match_index(3, Some(0)), Some(2));
}

#[test]
fn interruptible_search_supports_ascii_case_insensitive_matches() {
    let matches = search_text_interruptible(
        "Alpha alpha ALPHA",
        "alpha",
        SearchOptions::default(),
        || true,
    )
    .expect("search should complete");

    assert_eq!(matches.matches, vec![0..5, 6..11, 12..17]);
}

#[test]
fn case_insensitive_ascii_search_handles_single_byte_queries() {
    let matches = find_matches("AaA", "a", SearchOptions::default());
    assert_eq!(matches, vec![0..1, 1..2, 2..3]);
}

#[test]
fn interruptible_search_supports_case_sensitive_unicode_offsets() {
    let matches = search_text_interruptible(
        "naive cafe caf\u{00e9}",
        "caf\u{00e9}",
        SearchOptions {
            mode: SearchMode::PlainText,
            match_case: true,
            whole_word: false,
        },
        || true,
    )
    .expect("search should complete");

    assert_eq!(matches.matches, vec![11..15]);
}

#[test]
fn interruptible_ascii_search_can_cancel_mid_scan() {
    let text = "a".repeat(1024);
    let mut checks = 0;
    let result = search_text_interruptible(&text, "b", SearchOptions::default(), || {
        checks += 1;
        checks < 2
    });

    assert_eq!(result, None);
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
fn replace_all_changes_every_buffer_in_scope() {
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
    assert!(!app.replace_all_search_matches());
    assert_eq!(app.tabs()[0].active_buffer().text(), "alpha beta alpha");
    assert_eq!(app.tabs()[1].active_buffer().text(), "alpha gamma");
    assert!(app.replace_all_search_matches());
    assert_eq!(app.tabs()[0].active_buffer().text(), "omega beta omega");
    assert_eq!(app.tabs()[1].active_buffer().text(), "omega gamma");
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

#[test]
fn current_tab_search_moves_across_repeated_buffer_names_in_one_workspace_tab() {
    let mut app = test_app();
    app.tabs_mut()[0].buffer.name = "mod.rs".to_owned();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha first".to_owned());
    let first_view_id = app.tabs()[0].active_view_id;

    let second_view_id = app.tabs_mut()[0]
        .open_buffer_as_split(
            BufferState::new("mod.rs".to_owned(), "alpha second".to_owned(), None),
            SplitAxis::Vertical,
            false,
            0.5,
        )
        .expect("open split buffer");

    assert!(app.tabs_mut()[0].activate_view(first_view_id));
    let third_view_id = app.tabs_mut()[0]
        .open_buffer_as_split(
            BufferState::new("lib.rs".to_owned(), "alpha third".to_owned(), None),
            SplitAxis::Horizontal,
            false,
            0.5,
        )
        .expect("open split buffer");

    app.open_search();
    app.set_search_scope(SearchScope::ActiveWorkspaceTab);
    app.set_search_query("alpha");

    wait_for_search_matches(&mut app, 3);
    assert_eq!(app.search_match_count(), 3);
    assert_eq!(app.search_active_match_index(), Some(0));
    assert_eq!(app.tabs()[0].active_view_id, third_view_id);
    assert_eq!(app.tabs()[0].active_buffer().text(), "alpha third");

    assert!(app.select_next_search_match());
    assert_eq!(app.search_active_match_index(), Some(1));
    assert_eq!(app.tabs()[0].active_view_id, first_view_id);
    assert_eq!(app.tabs()[0].active_buffer().text(), "alpha first");

    assert!(app.select_next_search_match());
    assert_eq!(app.search_active_match_index(), Some(2));
    assert_eq!(app.tabs()[0].active_view_id, second_view_id);
    assert_eq!(app.tabs()[0].active_buffer().text(), "alpha second");
}
