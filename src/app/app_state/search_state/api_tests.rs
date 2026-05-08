use super::{SEARCH_PREVIEW_CACHE_LIMIT, ScratchpadApp, SearchMatch, SearchPreviewCacheKey};
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::search::find_matches;
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::SettingsStore;
use crate::app::startup::StartupOptions;
use std::ops::Range;
use std::time::{Duration, Instant};

#[test]
fn lazy_preview_lookup_builds_and_reuses_cached_entry() {
    let mut app = app_with_search_text("alpha\nplan beta\nomega");
    let plan_range = 6..10;
    seed_matches_for_plan_lines(&mut app, std::slice::from_ref(&plan_range));
    app.search_state.active_match_index = Some(0);

    let entry = app.search_result_entry_at(0).expect("preview entry");

    assert_eq!(entry.line_number, 2);
    assert_eq!(entry.column_number, 1);
    assert!(entry.preview.contains("plan beta"));
    assert!(entry.active);
    assert_eq!(app.search_state.preview_cache.len(), 1);

    app.search_state.active_match_index = None;
    let cached = app.search_result_entry_at(0).expect("cached preview entry");

    assert_eq!(cached.line_number, entry.line_number);
    assert_eq!(cached.preview, entry.preview);
    assert!(!cached.active);
    assert_eq!(app.search_state.preview_cache.len(), 1);
}

#[test]
fn lazy_preview_cache_evicts_least_recently_used_entry() {
    let text = (0..=SEARCH_PREVIEW_CACHE_LIMIT)
        .map(|_| "plan")
        .collect::<Vec<_>>()
        .join("\n");
    let ranges = text
        .match_indices("plan")
        .map(|(start, value)| start..start + value.len())
        .collect::<Vec<_>>();
    let mut app = app_with_search_text(&text);
    seed_matches_for_plan_lines(&mut app, &ranges);

    for index in 0..SEARCH_PREVIEW_CACHE_LIMIT {
        assert!(app.search_result_entry_at(index).is_some());
    }
    assert_eq!(
        app.search_state.preview_cache.len(),
        SEARCH_PREVIEW_CACHE_LIMIT
    );

    assert!(app.search_result_entry_at(0).is_some());
    assert!(
        app.search_result_entry_at(SEARCH_PREVIEW_CACHE_LIMIT)
            .is_some()
    );

    let generation = app.search_state.applied_generation;
    assert!(
        app.search_state
            .preview_cache
            .contains_key(&SearchPreviewCacheKey {
                generation,
                match_index: 0,
            })
    );
    assert!(
        !app.search_state
            .preview_cache
            .contains_key(&SearchPreviewCacheKey {
                generation,
                match_index: 1,
            })
    );
    assert_eq!(
        app.search_state.preview_cache.len(),
        SEARCH_PREVIEW_CACHE_LIMIT
    );
}

#[test]
fn replace_current_advances_past_self_matching_replacement() {
    let mut app = app_with_search_text("foo foo");
    seed_search_matches(&mut app, "foo");
    app.set_search_replacement("foobar");
    app.search_state.replace_open = true;
    app.search_state.active_match_index = Some(0);

    assert!(app.replace_current_search_match());
    wait_for_search_results(&mut app);

    assert_eq!(active_buffer_text(&app), "foobar foo");
    let active_match = active_search_match(&app).expect("active match after first replace");
    assert_eq!(active_match.range, 7..10);

    assert!(app.replace_current_search_match());
    wait_for_search_results(&mut app);

    assert_eq!(active_buffer_text(&app), "foobar foobar");
}

#[test]
fn replace_current_advances_after_empty_replacement() {
    let mut app = app_with_search_text("foo foo");
    seed_search_matches(&mut app, "foo");
    app.set_search_replacement("");
    app.search_state.replace_open = true;
    app.search_state.active_match_index = Some(0);

    assert!(app.replace_current_search_match());
    wait_for_search_results(&mut app);

    assert_eq!(active_buffer_text(&app), " foo");
    let active_match = active_search_match(&app).expect("active match after delete");
    assert_eq!(active_match.range, 1..4);
}

#[test]
fn tab_reorder_marks_open_search_dirty() {
    let mut app = app_with_search_text("first");
    app.append_tab(WorkspaceTab::new(BufferState::new(
        "second.md".to_owned(),
        "second".to_owned(),
        None,
    )));
    seed_search_matches(&mut app, "first");
    app.search_state.dirty = false;

    app.reorder_tab(0, 1);

    assert!(app.search_state.dirty);
    assert_eq!(app.search_state.freshness, super::SearchFreshness::Stale);
}

fn app_with_search_text(text: &str) -> ScratchpadApp {
    let temp_dir = tempfile::tempdir().expect("create temp app root");
    let root = temp_dir.keep();
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.clone()),
        SettingsStore::new(root),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    let tab = WorkspaceTab::new(BufferState::new(
        "search.md".to_owned(),
        text.to_owned(),
        None,
    ));
    app.tabs_mut()[0] = tab;
    app.search_state.applied_generation = 42;
    app
}

fn seed_matches_for_plan_lines(app: &mut ScratchpadApp, ranges: &[Range<usize>]) {
    let tab = &app.tabs()[0];
    let buffer = &tab.buffer;
    app.search_state.matches = ranges
        .iter()
        .cloned()
        .map(|range| SearchMatch {
            tab_index: 0,
            view_id: tab.active_view_id,
            buffer_id: buffer.id,
            buffer_label: buffer.display_name(),
            target_revision: buffer.document_revision(),
            matched_text: "plan".to_owned(),
            range,
        })
        .collect();
    app.search_state.total_match_count = app.search_state.matches.len();
    app.search_state.displayed_match_count = app.search_state.matches.len();
}

fn seed_search_matches(app: &mut ScratchpadApp, query: &str) {
    app.search_state.open = true;
    app.search_state.query = query.to_owned();
    app.search_state.status = super::SearchStatus::Ready;
    app.search_state.freshness = super::SearchFreshness::Fresh;
    app.search_state.searching = false;
    app.search_state.dirty = false;

    let text = active_buffer_text(app);
    let ranges = find_matches(&text, query, app.search_state.search_options());
    let tab = &app.tabs()[app.active_tab_index()];
    let buffer = &tab.buffer;
    app.search_state.matches = ranges
        .into_iter()
        .map(|range| SearchMatch {
            tab_index: app.active_tab_index(),
            view_id: tab.active_view_id,
            buffer_id: buffer.id,
            buffer_label: buffer.display_name(),
            target_revision: buffer.document_revision(),
            matched_text: text
                .chars()
                .skip(range.start)
                .take(range.end.saturating_sub(range.start))
                .collect(),
            range,
        })
        .collect();
    app.search_state.total_match_count = app.search_state.matches.len();
    app.search_state.displayed_match_count = app.search_state.matches.len();
}

fn active_buffer_text(app: &ScratchpadApp) -> String {
    app.active_tab()
        .and_then(|tab| tab.active_view())
        .and_then(|view| {
            app.active_tab()
                .and_then(|tab| tab.buffer_by_id(view.buffer_id))
        })
        .map(|buffer| buffer.text())
        .expect("active buffer text")
}

fn active_search_match(app: &ScratchpadApp) -> Option<&SearchMatch> {
    app.search_state
        .active_match_index
        .and_then(|index| app.search_state.matches.get(index))
}

fn wait_for_search_results(app: &mut ScratchpadApp) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        app.poll_search();
        if !app.search_state.searching && !app.search_state.dirty {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("search results did not settle");
}
