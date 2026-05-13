use super::{SEARCH_PREVIEW_CACHE_LIMIT, ScratchpadApp, SearchMatch, SearchPreviewCacheKey};
use crate::app::app_state::search_controller::{set_search_replace_open, set_search_replacement};
use crate::app::commands::AppCommand;
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
    app.state.search_state.results.active_match_index = Some(0);

    let entry = app.search_result_entry_at(0).expect("preview entry");

    assert_eq!(entry.line_number, 2);
    assert_eq!(entry.column_number, 1);
    assert!(entry.preview.contains("plan beta"));
    assert!(entry.active);
    assert_eq!(app.state.search_state.preview.entries.len(), 1);

    app.state.search_state.results.active_match_index = None;
    let cached = app.search_result_entry_at(0).expect("cached preview entry");

    assert_eq!(cached.line_number, entry.line_number);
    assert_eq!(cached.preview, entry.preview);
    assert!(!cached.active);
    assert_eq!(app.state.search_state.preview.entries.len(), 1);
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
        app.state.search_state.preview.entries.len(),
        SEARCH_PREVIEW_CACHE_LIMIT
    );

    assert!(app.search_result_entry_at(0).is_some());
    assert!(
        app.search_result_entry_at(SEARCH_PREVIEW_CACHE_LIMIT)
            .is_some()
    );

    let generation = app.state.search_state.runtime.applied_generation;
    assert!(
        app.state
            .search_state
            .preview
            .entries
            .contains_key(&SearchPreviewCacheKey {
                generation,
                match_index: 0,
            })
    );
    assert!(
        !app.state
            .search_state
            .preview
            .entries
            .contains_key(&SearchPreviewCacheKey {
                generation,
                match_index: 1,
            })
    );
    assert_eq!(
        app.state.search_state.preview.entries.len(),
        SEARCH_PREVIEW_CACHE_LIMIT
    );
}

#[test]
fn replace_current_advances_past_self_matching_replacement() {
    let mut app = app_with_search_text("foo foo");
    seed_search_matches(&mut app, "foo");
    set_search_replacement(&mut app, "foobar");
    app.state.search_state.panel.replace_open = true;
    app.state.search_state.results.active_match_index = Some(0);

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
    set_search_replacement(&mut app, "");
    app.state.search_state.panel.replace_open = true;
    app.state.search_state.results.active_match_index = Some(0);

    assert!(app.replace_current_search_match());
    wait_for_search_results(&mut app);

    assert_eq!(active_buffer_text(&app), " foo");
    let active_match = active_search_match(&app).expect("active match after delete");
    assert_eq!(active_match.range, 1..4);
}

#[test]
fn tab_reorder_marks_open_search_dirty() {
    let mut app = app_with_search_text("first");
    crate::app::app_state::workspace_controller::append_tab(
        &mut app,
        WorkspaceTab::new(BufferState::new(
            "second.md".to_owned(),
            "second".to_owned(),
            None,
        )),
    );
    seed_search_matches(&mut app, "first");
    app.state.search_state.runtime.dirty = false;

    app.handle_command(AppCommand::ReorderTab {
        from_index: 0,
        to_index: 1,
    });

    assert!(app.state.search_state.runtime.dirty);
    assert_eq!(
        app.state.search_state.runtime.freshness,
        super::SearchFreshness::Stale
    );
}

#[test]
fn replacement_typing_updates_preview_without_mutating_buffer() {
    let mut app = app_with_search_text("foo foo");
    seed_search_matches(&mut app, "foo");
    app.state.search_state.panel.replace_open = true;
    let before_revision = active_buffer_revision(&app);

    set_search_replacement(&mut app, "bar");

    assert_eq!(active_buffer_text(&app), "foo foo");
    assert_eq!(active_buffer_revision(&app), before_revision);
    let preview = active_view_replacement_preview(&app).expect("replacement preview");
    assert_eq!(preview.entries.len(), 2);
    assert_eq!(preview.entries[0].range, 0..3);
    assert_eq!(preview.entries[0].replacement, "bar");
    assert_eq!(preview.entries[1].range, 4..7);
    assert_eq!(preview.entries[1].replacement, "bar");
}

#[test]
fn closing_replace_mode_clears_replacement_preview() {
    let mut app = app_with_search_text("foo");
    seed_search_matches(&mut app, "foo");
    app.state.search_state.panel.replace_open = true;
    set_search_replacement(&mut app, "bar");
    assert!(active_view_replacement_preview(&app).is_some());

    set_search_replace_open(&mut app, false);

    assert!(active_view_replacement_preview(&app).is_none());
    assert_eq!(active_buffer_text(&app), "foo");
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
    app.tab_manager.tabs.as_mut_slice()[0] = tab;
    app.state.search_state.runtime.applied_generation = 42;
    app
}

fn seed_matches_for_plan_lines(app: &mut ScratchpadApp, ranges: &[Range<usize>]) {
    let tab = &app.tab_manager.tabs.as_slice()[0];
    let buffer = &tab.buffer;
    app.state.search_state.results.matches = ranges
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
    app.state.search_state.results.total_match_count = app.state.search_state.results.matches.len();
    app.state.search_state.results.displayed_match_count =
        app.state.search_state.results.matches.len();
}

fn seed_search_matches(app: &mut ScratchpadApp, query: &str) {
    app.state.search_state.panel.open = true;
    app.state.search_state.query.query = query.to_owned();
    app.state.search_state.runtime.status = super::SearchStatus::Ready;
    app.state.search_state.runtime.freshness = super::SearchFreshness::Fresh;
    app.state.search_state.runtime.searching = false;
    app.state.search_state.runtime.dirty = false;

    let text = active_buffer_text(app);
    let ranges = find_matches(&text, query, app.state.search_state.search_options());
    let tab = &app.tab_manager.tabs.as_slice()[app.tab_manager.active_tab_index];
    let buffer = &tab.buffer;
    app.state.search_state.results.matches = ranges
        .into_iter()
        .map(|range| SearchMatch {
            tab_index: app.tab_manager.active_tab_index,
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
    app.state.search_state.results.total_match_count = app.state.search_state.results.matches.len();
    app.state.search_state.results.displayed_match_count =
        app.state.search_state.results.matches.len();
}

fn active_buffer_text(app: &ScratchpadApp) -> String {
    app.tab_manager
        .active_tab()
        .and_then(|tab| tab.active_view())
        .and_then(|view| {
            app.tab_manager
                .active_tab()
                .and_then(|tab| tab.buffer_by_id(view.buffer_id))
        })
        .map(|buffer| buffer.text())
        .expect("active buffer text")
}

fn active_buffer_revision(app: &ScratchpadApp) -> u64 {
    app.tab_manager
        .active_tab()
        .and_then(|tab| tab.active_view())
        .and_then(|view| {
            app.tab_manager
                .active_tab()
                .and_then(|tab| tab.buffer_by_id(view.buffer_id))
        })
        .map(|buffer| buffer.document_revision())
        .expect("active buffer revision")
}

fn active_view_replacement_preview(
    app: &ScratchpadApp,
) -> Option<&crate::app::domain::SearchReplacementPreview> {
    app.tab_manager
        .active_tab()
        .and_then(|tab| tab.active_view())
        .and_then(|view| view.search_replacement_preview.as_ref())
}

fn active_search_match(app: &ScratchpadApp) -> Option<&SearchMatch> {
    app.state
        .search_state
        .results
        .active_match_index
        .and_then(|index| app.state.search_state.results.matches.get(index))
}

fn wait_for_search_results(app: &mut ScratchpadApp) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        app.poll_search();
        if !app.state.search_state.runtime.searching && !app.state.search_state.runtime.dirty {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("search results did not settle");
}
