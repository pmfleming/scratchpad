use super::{SEARCH_PREVIEW_CACHE_LIMIT, ScratchpadApp, SearchMatch, SearchPreviewCacheKey};
use crate::app::app_state::test_support::test_app;
use crate::app::commands::{AppCommand, SearchCommand, WorkspaceCommand};
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::search::find_matches;
use crate::app::ui::editor_content::native_editor::CursorRange;
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
    set_replacement(&mut app, "foobar");
    app.state.search_state.panel.replace_open = true;
    app.state.search_state.results.active_match_index = Some(0);

    assert!(search(&mut app, SearchCommand::ReplaceCurrentMatch));
    wait_for_search_results(&mut app);

    assert_eq!(active_buffer_text(&app), "foobar foo");
    let active_match = active_search_match(&app).expect("active match after first replace");
    assert_eq!(active_match.range, 7..10);

    assert!(search(&mut app, SearchCommand::ReplaceCurrentMatch));
    wait_for_search_results(&mut app);

    assert_eq!(active_buffer_text(&app), "foobar foobar");
}

#[test]
fn replace_current_advances_after_empty_replacement() {
    let mut app = app_with_search_text("foo foo");
    seed_search_matches(&mut app, "foo");
    set_replacement(&mut app, "");
    app.state.search_state.panel.replace_open = true;
    app.state.search_state.results.active_match_index = Some(0);

    assert!(search(&mut app, SearchCommand::ReplaceCurrentMatch));
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

    crate::app::commands::handle_command(
        &mut app,
        AppCommand::Workspace(WorkspaceCommand::ReorderTab {
            from_index: 0,
            to_index: 1,
        }),
    );

    assert!(app.state.search_state.runtime.dirty);
    assert_eq!(
        app.state.search_state.runtime.freshness,
        super::SearchFreshness::Stale
    );
}

#[test]
fn active_buffer_search_counts_matches() {
    let mut app = app_with_search_text("foo bar foo");

    assert!(search(&mut app, SearchCommand::Open));
    set_query(&mut app, "foo");
    wait_for_search_results(&mut app);

    for command in [
        SearchCommand::SetSearchMode {
            mode: crate::app::services::search::SearchMode::Regex,
        },
        SearchCommand::SetSearchMatchCase { enabled: true },
        SearchCommand::SetSearchWholeWord { enabled: true },
    ] {
        assert!(search(&mut app, command));
    }
    wait_for_search_results(&mut app);
    assert!(app.state.search_state.query.match_case && app.state.search_state.query.whole_word);
    assert_eq!(
        app.state.search_state.query.mode,
        crate::app::services::search::SearchMode::Regex
    );
    assert!(search(&mut app, SearchCommand::NextSearchMatch));
    assert_eq!(app.search_active_match_index(), Some(1));
    assert!(search(&mut app, SearchCommand::PreviousSearchMatch));
    let view = app
        .tab_manager
        .active_tab()
        .unwrap()
        .layout
        .active_view_id();
    for (command, expected) in [
        (
            SearchCommand::FocusSearchResultFile { match_index: 1 },
            true,
        ),
        (SearchCommand::ActivateSearchMatch { match_index: 0 }, true),
        (
            SearchCommand::FocusSearchResultFile {
                match_index: usize::MAX,
            },
            false,
        ),
        (
            SearchCommand::ActivateSearchMatch {
                match_index: usize::MAX,
            },
            false,
        ),
    ] {
        app.state.focus.clear();
        assert_eq!(search(&mut app, command), expected);
        assert_eq!(
            crate::app::app_state::workspace::accessors::should_focus_view(&app, view),
            expected
        );
    }
    assert_eq!(app.search_match_count(), 2);
    assert_eq!(app.search_active_match_index(), Some(0));
}

#[test]
fn all_open_tabs_search_counts_matches_across_tabs() {
    let mut app = app_with_second_tab("needle one", "needle two needle");

    assert!(search(&mut app, SearchCommand::Open));
    assert!(search(
        &mut app,
        SearchCommand::SetSearchScope {
            scope: super::SearchScope::AllOpenTabs
        }
    ));
    set_query(&mut app, "needle");
    wait_for_search_results(&mut app);

    assert_eq!(app.search_match_count(), 3);
}

#[test]
fn replace_all_changes_every_buffer_in_scope() {
    let mut app = app_with_second_tab("foo one", "foo two foo");

    assert!(search(&mut app, SearchCommand::OpenAndReplace));
    assert!(search(
        &mut app,
        SearchCommand::SetSearchScope {
            scope: super::SearchScope::AllOpenTabs
        }
    ));
    set_query(&mut app, "foo");
    set_replacement(&mut app, "bar");
    wait_for_search_results(&mut app);

    assert!(!search(&mut app, SearchCommand::ReplaceAllMatches));
    assert!(search(&mut app, SearchCommand::ReplaceAllMatches));
    wait_for_search_results(&mut app);

    assert_eq!(
        app.tab_manager.tabs.as_slice()[0].buffers.buffer.text(),
        "bar one"
    );
    assert_eq!(
        app.tab_manager.tabs.as_slice()[1].buffers.buffer.text(),
        "bar two bar"
    );
}

#[test]
fn replacement_typing_updates_preview_without_mutating_buffer() {
    let mut app = app_with_search_text("foo foo");
    seed_search_matches(&mut app, "foo");
    app.state.search_state.panel.replace_open = true;
    let before_revision = active_buffer_revision(&app);

    set_replacement(&mut app, "bar");

    assert_eq!(active_buffer_text(&app), "foo foo");
    assert_eq!(active_buffer_revision(&app), before_revision);
    let preview = active_view_replacement_preview(&app).expect("replacement preview");
    assert_eq!(preview.entries().len(), 2);
    assert_eq!(preview.entries()[0].range, 0..3);
    assert_eq!(preview.entries()[0].replacement, "bar");
    assert_eq!(preview.entries()[1].range, 4..7);
    assert_eq!(preview.entries()[1].replacement, "bar");
}

#[test]
fn closing_replace_mode_clears_replacement_preview() {
    let mut app = app_with_search_text("foo");
    seed_search_matches(&mut app, "foo");
    app.state.search_state.panel.replace_open = true;
    set_replacement(&mut app, "bar");
    assert!(active_view_replacement_preview(&app).is_some());

    assert!(search(
        &mut app,
        SearchCommand::SetSearchReplaceOpen { open: false }
    ));

    assert!(active_view_replacement_preview(&app).is_none());
    assert_eq!(active_buffer_text(&app), "foo");
    for (command, open, replace) in [
        (SearchCommand::Close, false, false),
        (SearchCommand::Toggle, true, false),
        (SearchCommand::Toggle, false, false),
        (SearchCommand::OpenAndReplace, true, true),
    ] {
        assert!(search(&mut app, command));
        assert_eq!(app.state.search_state.panel.open, open);
        assert_eq!(app.state.search_state.panel.replace_open, replace);
    }
    assert!(search(
        &mut app,
        SearchCommand::SetSearchReplaceOpen { open: true }
    ));
}

fn set_replacement(app: &mut ScratchpadApp, value: &str) {
    assert!(search(
        app,
        SearchCommand::SetSearchReplacement {
            replacement: value.into()
        }
    ));
}

fn set_query(app: &mut ScratchpadApp, value: &str) {
    assert!(search(
        app,
        SearchCommand::SetSearchQuery {
            query: value.into()
        }
    ));
}

fn search(app: &mut ScratchpadApp, command: SearchCommand) -> bool {
    crate::app::commands::handle_command(app, AppCommand::Search(command))
}

fn app_with_search_text(text: &str) -> ScratchpadApp {
    let temp_dir = tempfile::tempdir().expect("create temp app root");
    let root = temp_dir.keep();
    let mut app = test_app(&root);
    let tab = WorkspaceTab::new(BufferState::new(
        "search.md".to_owned(),
        text.to_owned(),
        None,
    ));
    app.tab_manager.tabs.as_mut_slice()[0] = tab;
    app.state.search_state.runtime.applied_generation = 42;
    app
}

fn app_with_second_tab(first: &str, second: &str) -> ScratchpadApp {
    let mut app = app_with_search_text(first);
    app.tab_manager.tabs.as_mut_slice()[0].buffers.buffer.name = "one.txt".to_owned();
    crate::app::app_state::workspace_controller::append_tab(
        &mut app,
        WorkspaceTab::new(BufferState::new(
            "two.txt".to_owned(),
            second.to_owned(),
            None,
        )),
    );
    app
}

fn start_replace(app: &mut ScratchpadApp, query: &str, replacement: &str) {
    super::api::open_search_and_replace(app);
    super::api::set_search_query(app, query);
    super::api::set_search_replacement(app, replacement);
}

fn skip_replace_review(app: &mut ScratchpadApp) {
    let unlimited = crate::app::services::settings_store::ReviewThreshold::UNLIMITED;
    app.state.app_settings.search.review_files_above = unlimited;
    app.state.app_settings.search.review_matches_above = unlimited;
}

fn poll_until(
    app: &mut ScratchpadApp,
    attempts: usize,
    poll: fn(&mut ScratchpadApp),
    done: fn(&ScratchpadApp) -> bool,
) {
    for _ in 0..attempts {
        poll(app);
        if done(app) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(done(app));
}

fn set_matches(app: &mut ScratchpadApp, tab_index: usize, ranges: Vec<Range<usize>>) {
    let tab = &app.tab_manager.tabs.as_slice()[tab_index];
    let buffer = &tab.buffers.buffer;
    let results = &mut app.state.search_state.results;
    results.matches = ranges
        .into_iter()
        .map(|range| SearchMatch {
            tab_index,
            view_id: tab.layout.active_view_id,
            buffer_id: buffer.id,
            buffer_label: buffer.display_name().into(),
            target_revision: buffer.document_revision(),
            target_identity: buffer.document_snapshot().revision_key(),
            matched_text: None,
            range,
        })
        .collect();
    results.total_match_count = results.matches.len();
    results.displayed_match_count = results.matches.len();
}

fn seed_matches_for_plan_lines(app: &mut ScratchpadApp, ranges: &[Range<usize>]) {
    set_matches(app, 0, ranges.to_vec());
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
    set_matches(app, app.tab_manager.active_tab_index, ranges);
}

fn active_buffer(app: &ScratchpadApp) -> &BufferState {
    let tab = app.tab_manager.active_tab().expect("active tab");
    tab.layout
        .active_view()
        .and_then(|view| tab.buffer_by_id(view.buffer_id))
        .expect("active buffer")
}

fn active_buffer_text(app: &ScratchpadApp) -> String {
    active_buffer(app).text()
}

fn active_buffer_revision(app: &ScratchpadApp) -> u64 {
    active_buffer(app).document_revision()
}

fn active_view_replacement_preview(
    app: &ScratchpadApp,
) -> Option<&crate::app::domain::SearchReplacementPreview> {
    app.tab_manager
        .active_tab()
        .and_then(|tab| tab.layout.active_view())
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

#[test]
#[ignore = "manual performance measurement"]
fn replacement_navigation_performance() {
    for count in [1_000, 16_000] {
        let mut app = app_with_search_text(&"foo ".repeat(count));
        let tab = app.tab_manager.active_tab_mut().unwrap();
        for _ in 0..3 {
            tab.split_active_view(crate::app::domain::SplitAxis::Horizontal);
        }
        seed_search_matches(&mut app, "foo");
        app.state.search_state.panel.replace_open = true;
        app.state.search_state.query.replacement = "replacement".into();
        super::visual::set_active_search_index(&mut app, Some(0));
        let mut samples = Vec::new();
        for index in 1..=7 {
            let start = Instant::now();
            super::visual::set_active_search_index(&mut app, Some(index));
            samples.push(start.elapsed().as_nanos());
        }
        samples.sort_unstable();
        println!(
            "replacement_navigation matches={count} views=4 median_ns={} range={}-{}",
            samples[3], samples[0], samples[6]
        );
    }
}

#[test]
fn replacement_previews_share_storage_and_invalidate_on_edits_and_options() {
    let mut app = app_with_search_text("foo foo");
    app.tab_manager
        .active_tab_mut()
        .unwrap()
        .split_active_view(crate::app::domain::SplitAxis::Horizontal);
    seed_search_matches(&mut app, "foo");
    app.state.search_state.panel.replace_open = true;
    app.state.search_state.query.replacement = "bar".into();
    super::visual::set_active_search_index(&mut app, Some(0));
    let initial = active_view_replacement_preview(&app).unwrap().clone();
    for view in app.tab_manager.active_tab().unwrap().layout.views() {
        assert_eq!(
            view.search_replacement_preview
                .as_ref()
                .unwrap()
                .entries()
                .as_ptr(),
            initial.entries().as_ptr()
        );
    }
    let ranges = app
        .tab_manager
        .active_tab()
        .unwrap()
        .layout
        .active_view()
        .unwrap()
        .search_highlights
        .ranges
        .as_ptr();
    super::visual::set_active_search_index(&mut app, Some(1));
    assert_eq!(
        active_view_replacement_preview(&app)
            .unwrap()
            .entries()
            .as_ptr(),
        initial.entries().as_ptr()
    );
    assert_eq!(
        app.tab_manager
            .active_tab()
            .unwrap()
            .layout
            .active_view()
            .unwrap()
            .search_highlights
            .ranges
            .as_ptr(),
        ranges
    );
    app.state.search_state.query.replacement = "baz".into();
    super::visual::refresh_search_visual_state(&mut app);
    let replacement = active_view_replacement_preview(&app).unwrap().clone();
    assert_eq!(replacement.entries()[0].replacement, "baz");
    assert_ne!(replacement.entries().as_ptr(), initial.entries().as_ptr());
    app.tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut()
        .document_mut()
        .insert_direct(0, " ");
    seed_search_matches(&mut app, "foo");
    super::visual::refresh_search_visual_state(&mut app);
    let edited = active_view_replacement_preview(&app).unwrap();
    assert_eq!(edited.entries()[0].range, 1..4);
    assert_ne!(edited.entries().as_ptr(), replacement.entries().as_ptr());
    app.toggle_search();
    assert!(active_view_replacement_preview(&app).is_none());
}

#[test]
fn context_sensitive_regex_replacements_and_case_preservation() {
    for (text, pattern, replacement, expected) in [
        ("foo foo", r"\B(o{2})", "$1!", "foo! foo!"),
        ("é foo", r"\b(foo)", "${1}x", "é foox"),
        ("foo\nfoo", r"(?m)^(foo)$", "<$1>", "<foo>\n<foo>"),
    ] {
        let mut app = app_with_search_text(text);
        super::api::open_search_and_replace(&mut app);
        super::api::set_search_mode(&mut app, crate::app::services::search::SearchMode::Regex);
        super::api::set_search_query(&mut app, pattern);
        super::api::set_search_replacement(&mut app, replacement);
        wait_for_search_results(&mut app);
        assert!(app.replace_all_search_matches());
        wait_for_search_results(&mut app);
        assert_eq!(active_buffer_text(&app), expected, "{pattern}");
    }
    let mut app = app_with_search_text("needle Needle NEEDLE nEeDlE");
    super::api::open_search_and_replace(&mut app);
    super::api::set_search_query(&mut app, "needle");
    super::api::set_search_replacement(&mut app, "thread");
    app.state.search_state.query.preserve_case = true;
    wait_for_search_results(&mut app);
    assert!(app.replace_all_search_matches());
    wait_for_search_results(&mut app);
    assert_eq!(active_buffer_text(&app), "thread Thread THREAD thread");
}

#[test]
fn locked_selection_survives_navigation_and_replacement() {
    let mut app = app_with_search_text("foo foo foo");
    let tab = app.tab_manager.active_tab_mut().unwrap();
    let id = tab.layout.active_view_id();
    let (buffer, view) = tab.buffer_and_view_mut(id).unwrap();
    view.set_cursor_range_anchored(buffer, crate::app::domain::CursorRange::two(0, 7));
    super::api::open_search_and_replace(&mut app);
    super::api::set_search_query(&mut app, "foo");
    super::api::set_search_replacement(&mut app, "longer");
    wait_for_search_results(&mut app);
    assert_eq!(app.search_match_count(), 2);
    assert_eq!(super::selection::resolve(&app).unwrap().3, 0..7);
    assert!(super::replace::replace_current_search_match(&mut app));
    wait_for_search_results(&mut app);
    assert_eq!(active_buffer_text(&app), "longer foo foo");
    assert_eq!(super::selection::resolve(&app).unwrap().3, 0..10);
    assert_eq!(app.search_match_count(), 1);
    assert!(app.replace_all_search_matches());
    wait_for_search_results(&mut app);
    assert_eq!(active_buffer_text(&app), "longer longer foo");
    assert_eq!(super::selection::resolve(&app).unwrap().3, 0..13);
}

#[test]
fn review_holds_exact_plan_exclusions_and_grouped_undo() {
    use crate::app::services::session_store::activity::ActivityKind;
    let mut app = app_with_search_text("foo foo foo");
    start_replace(&mut app, "foo", "bar");
    wait_for_search_results(&mut app);
    app.state.app_settings.search.review_matches_above =
        crate::app::services::settings_store::ReviewThreshold::Count(0);
    app.state.search_state.replace.excluded.insert(1);
    assert!(!app.replace_all_search_matches());
    let review = app
        .state
        .search_state
        .replace
        .pending_replace_all_confirmation
        .as_ref()
        .unwrap();
    assert_eq!(review.total_match_count, 2);
    assert_eq!(review.plan.targets[0].replacements.len(), 2);
    assert!(app.replace_all_search_matches());
    wait_for_search_results(&mut app);
    assert_eq!(active_buffer_text(&app), "bar foo bar");
    let (operation, files) = app.state.search_state.replace.last_operation.unwrap();
    assert_eq!(files, 1);
    assert!(app.state.persistence.pending_activity.iter().any(|e| matches!(e.kind, ActivityKind::ReplaceAll { operation_id, matches: 2, .. } if operation_id == operation)));
    assert!(super::replace::transaction::undo(
        &mut app, operation, files
    ));
    assert_eq!(active_buffer_text(&app), "foo foo foo");
}

#[test]
fn unlimited_thresholds_skip_review_but_stale_preparation_changes_nothing() {
    let mut app = app_with_search_text(&"foo ".repeat(400));
    start_replace(&mut app, "foo", "bar");
    skip_replace_review(&mut app);
    wait_for_search_results(&mut app);
    assert!(app.replace_all_search_matches());
    assert!(app.state.search_state.replace.pending.is_some());
    assert!(app.state.search_state.replace.last_operation.is_none());
    let buffer = &mut app.tab_manager.active_tab_mut().unwrap().buffers.buffer;
    buffer
        .replace_char_ranges_with_undo(
            &[(0..3, "edit".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    poll_until(&mut app, 500, super::replace::transaction::poll, |app| {
        app.state.search_state.replace.pending.is_none()
    });
    assert!(active_buffer_text(&app).starts_with("edit foo"));
    assert!(!active_buffer_text(&app).contains("bar"));
    assert!(app.state.search_state.replace.last_operation.is_none());
}

#[test]
fn result_limit_keeps_bounded_previews_and_reviews_every_replacement() {
    let mut app = app_with_search_text(&"foo ".repeat(50_002));
    app.state.app_settings.search.result_limit =
        crate::app::services::settings_store::SearchResultLimit::from_matches(50_000);
    skip_replace_review(&mut app);
    start_replace(&mut app, "foo", "bar");
    wait_for_search_results(&mut app);
    assert_eq!(app.search_match_count(), 50_000);
    assert!(matches!(
        app.state.search_state.runtime.status,
        super::SearchStatus::Error(_)
    ));
    assert!(app.replace_all_search_matches());
    poll_until(&mut app, 30000, super::replace::limited::poll, |app| {
        app.state.search_state.replace.planning.is_none()
    });
    assert_eq!(
        app.state
            .search_state
            .replace
            .pending_replace_all_confirmation
            .as_ref()
            .unwrap()
            .total_match_count,
        50_002
    );
    assert!(active_buffer_text(&app).starts_with("foo"));
    assert!(app.replace_all_search_matches());
    poll_until(&mut app, 30000, super::replace::transaction::poll, |app| {
        app.state.search_state.replace.pending.is_none()
    });
    assert_eq!(active_buffer_text(&app), "bar ".repeat(50_002));
}

#[test]
fn multi_file_replace_is_atomic_and_undo_refuses_to_discard_later_work() {
    let mut app = app_with_second_tab("foo first", "foo second");
    start_replace(&mut app, "foo", "bar");
    super::api::set_search_scope(&mut app, super::SearchScope::AllOpenTabs);
    wait_for_search_results(&mut app);
    assert!(!app.replace_all_search_matches());
    assert!(app.replace_all_search_matches());
    let (operation, files) = app.state.search_state.replace.last_operation.unwrap();
    assert!(super::replace::transaction::can_undo(
        &app, operation, files
    ));
    assert!(super::replace::transaction::undo(
        &mut app, operation, files
    ));
    assert_eq!(
        app.tab_manager.tabs.as_slice()[0].active_buffer().text(),
        "foo first"
    );
    assert_eq!(
        app.tab_manager.tabs.as_slice()[1].active_buffer().text(),
        "foo second"
    );
    wait_for_search_results(&mut app);
    assert!(!app.replace_all_search_matches());
    assert!(app.replace_all_search_matches());
    let (operation, files) = app.state.search_state.replace.last_operation.unwrap();
    app.tab_manager.tabs.as_mut_slice()[1]
        .active_buffer_mut()
        .replace_char_ranges_with_undo(
            &[(0..3, "new".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    assert!(!super::replace::transaction::undo(
        &mut app, operation, files
    ));
    assert_eq!(
        app.tab_manager.tabs.as_slice()[0].active_buffer().text(),
        "bar first"
    );
    assert_eq!(
        app.tab_manager.tabs.as_slice()[1].active_buffer().text(),
        "new second"
    );
}

#[test]
fn stale_replacement_preview_rejects_old_identity_and_out_of_scope_ranges() {
    let mut app = app_with_search_text("foo foo");
    seed_search_matches(&mut app, "foo");
    let matched = app.state.search_state.results.matches[0].clone();
    let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
    let program = crate::app::services::search::SearchProgram::compile(
        "foo",
        app.state.search_state.search_options(),
    )
    .unwrap();
    assert!(
        super::replace::expand_for_buffer_in_range(
            &program,
            buffer,
            &(0..3),
            "bar",
            false,
            Some(4..7)
        )
        .is_err()
    );
    app.tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut()
        .replace_text("different".into());
    assert!(super::replace::replacement_for_match(&app, &matched).is_err());
}
