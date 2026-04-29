use super::helpers::cursor_range_from_char_range;
use super::{
    ScratchpadApp, SearchReplaceAvailability, SearchScope, SearchScopeOrigin, SearchStatus,
};
use crate::app::commands::AppCommand;
use crate::app::domain::buffer::PieceTreeLite;
use crate::app::domain::{BufferState, CursorRevealMode, SearchHighlightState, SplitAxis};
use crate::app::services::search::SearchMode;
use crate::app::services::session_store::SessionStore;
use crate::app::ui::scrolling::{
    ContentExtent, DisplaySnapshot, ScrollAlign, ScrollIntent, ViewportMetrics,
};
use eframe::egui;
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

fn wait_for_search_condition(
    app: &mut ScratchpadApp,
    predicate: impl Fn(&ScratchpadApp) -> bool,
    description: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        app.poll_search();
        if predicate(app) {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out waiting for search state: {description}");
}

fn snapshot_for(text: &str) -> DisplaySnapshot {
    let ctx = egui::Context::default();
    let mut galley = None;
    let _ = ctx.run_ui(Default::default(), |ui| {
        galley = Some(ui.fonts_mut(|fonts| {
            fonts.layout_job(egui::text::LayoutJob::simple(
                text.to_owned(),
                egui::FontId::monospace(14.0),
                egui::Color32::WHITE,
                f32::INFINITY,
            ))
        }));
    });
    DisplaySnapshot::from_galley(galley.expect("galley"), 10.0)
}

fn install_snapshot_on_active_view(app: &mut ScratchpadApp, snapshot: DisplaySnapshot) {
    let view = app.tabs_mut()[0].active_view_mut().expect("active view");
    view.scroll.set_metrics(ViewportMetrics {
        viewport_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 40.0)),
        row_height: snapshot.row_height(),
        column_width: 5.0,
        visible_rows: 4,
        visible_columns: 40,
    });
    view.scroll.set_extent(ContentExtent {
        display_rows: snapshot.row_count(),
        height: snapshot.content_height(),
        max_line_width: snapshot.max_line_width(),
    });
    view.latest_display_snapshot = Some(snapshot);
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
    assert_eq!(groups[0].buffer_label, "Untitled");
    assert_eq!(groups[0].total_match_count, 1);
    assert_eq!(groups[1].buffer_label, "Untitled");
    assert_eq!(groups[1].total_match_count, 1);
}

#[test]
fn search_result_groups_are_separated_by_file_within_a_tab() {
    let mut app = test_app();
    app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
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
        .expect("open buffer split should succeed");

    app.open_search();
    app.set_search_scope(SearchScope::ActiveWorkspaceTab);
    app.set_search_query("alpha");

    wait_for_search_matches(&mut app, 2);

    let groups = app.search_result_groups();
    assert_eq!(groups.len(), 2);
    let labels = groups
        .iter()
        .map(|group| group.buffer_label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["two.txt", "one.txt"]);
    assert!(groups.iter().all(|group| group.total_match_count == 1));
}

#[test]
fn dirty_single_buffer_results_use_dirty_buffer_label_without_extra_context_split() {
    let mut app = test_app();
    app.tabs_mut()[0].buffer.name = "notes.txt".to_owned();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha only here".to_owned());
    app.tabs_mut()[0].buffer.is_dirty = true;

    app.open_search();
    app.set_search_scope(SearchScope::ActiveBuffer);
    app.set_search_query("alpha");

    wait_for_search_matches(&mut app, 1);

    let groups = app.search_result_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].entries.len(), 1);
    assert_eq!(groups[0].entries[0].buffer_label, "*notes.txt");
    assert_eq!(groups[0].tab_label, "*notes.txt");
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
    app.clear_session_dirty();

    app.open_search();
    app.set_search_scope(SearchScope::AllOpenTabs);
    app.set_search_query("alpha");
    wait_for_search_matches(&mut app, 1);

    assert!(app.activate_search_match_at(0));
    assert_eq!(app.active_tab_index(), 1);
    assert!(!app.session_dirty());
    let pending = app
        .active_tab()
        .and_then(|tab| tab.active_view())
        .and_then(|view| view.pending_cursor_range);
    assert_eq!(pending, Some(cursor_range_from_char_range(0..5)));
    assert!(
        app.active_tab()
            .and_then(|tab| tab.active_view())
            .is_some_and(|view| view.cursor_reveal_mode().is_some())
    );
}

#[test]
fn activating_search_match_queues_scroll_reveal_when_snapshot_exists() {
    let mut app = test_app();
    let text = format!("{}alpha target\n", "preface\n".repeat(30));
    app.tabs_mut()[0].buffer.replace_text(text.clone());

    app.open_search();
    app.set_search_query("alpha");
    wait_for_search_matches(&mut app, 1);
    install_snapshot_on_active_view(&mut app, snapshot_for(&text));

    assert!(app.activate_search_match_at(0));

    let view = app.tabs()[0].active_view().expect("active view");
    assert!(view.pending_intents.iter().any(|intent| matches!(
        intent,
        ScrollIntent::Reveal {
            align_y: Some(ScrollAlign::Center),
            align_x: None,
            ..
        }
    )));
    assert_eq!(
        view.cursor_reveal_mode(),
        Some(CursorRevealMode::KeepHorizontalVisible)
    );
}

#[test]
fn activating_search_match_uses_first_tile_for_duplicate_buffer_results() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha target".to_owned());

    let first_view_id = app.tabs()[0].active_view_id;
    let second_view_id = app.tabs_mut()[0]
        .split_active_view(SplitAxis::Vertical)
        .expect("split active view");
    assert!(app.tabs_mut()[0].activate_view(second_view_id));

    app.open_search();
    app.set_search_scope(SearchScope::ActiveWorkspaceTab);
    app.set_search_query("alpha");
    wait_for_search_matches(&mut app, 1);

    assert!(app.activate_search_match_at(0));
    assert_eq!(app.tabs()[0].active_view_id, first_view_id);
    let pending = app.tabs()[0]
        .view(first_view_id)
        .and_then(|view| view.pending_cursor_range);
    assert_eq!(pending, Some(cursor_range_from_char_range(0..5)));
    assert!(
        app.tabs()[0]
            .view(first_view_id)
            .is_some_and(|view| view.cursor_reveal_mode().is_some())
    );
}

#[test]
fn focusing_search_result_file_uses_first_tile_without_selecting_a_match() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha target".to_owned());

    let first_view_id = app.tabs()[0].active_view_id;
    let second_view_id = app.tabs_mut()[0]
        .split_active_view(SplitAxis::Vertical)
        .expect("split active view");
    assert!(app.tabs_mut()[0].activate_view(second_view_id));
    app.clear_session_dirty();

    app.open_search();
    app.set_search_scope(SearchScope::ActiveWorkspaceTab);
    app.set_search_query("alpha");
    wait_for_search_matches(&mut app, 1);
    let active_match_index = app.search_active_match_index();

    assert!(app.focus_search_result_file_at(0));
    assert_eq!(app.tabs()[0].active_view_id, first_view_id);
    assert_eq!(app.search_active_match_index(), active_match_index);
    let pending = app.tabs()[0]
        .view(first_view_id)
        .and_then(|view| view.pending_cursor_range);
    assert_eq!(pending, None);
    assert!(!app.session_dirty());
}

#[test]
fn focusing_search_result_file_prefers_first_visible_tile_for_duplicate_buffers() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha target".to_owned());

    let original_view_id = app.tabs()[0].active_view_id;
    let leading_view_id = app.tabs_mut()[0]
        .split_active_view_with_placement(SplitAxis::Vertical, true, 0.5)
        .expect("split active view with leading placement");
    assert_ne!(original_view_id, leading_view_id);
    assert!(app.tabs_mut()[0].activate_view(original_view_id));

    app.open_search();
    app.set_search_scope(SearchScope::ActiveWorkspaceTab);
    app.set_search_query("alpha");
    wait_for_search_matches(&mut app, 1);

    assert!(app.focus_search_result_file_at(0));
    assert_eq!(app.tabs()[0].active_view_id, leading_view_id);
}

#[test]
fn preview_for_match_reports_line_and_column() {
    let tree = PieceTreeLite::from_string("one\ntwo alpha\nthree".to_owned());
    let (line, column, preview) = tree.preview_for_match(&(8..13));
    assert_eq!(line, 2);
    assert_eq!(column, 5);
    assert_eq!(preview, "two alpha");
}

#[test]
fn open_search_defaults_to_selection_scope_when_selection_exists() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha beta alpha".to_owned());
    app.tabs_mut()[0]
        .active_view_mut()
        .expect("active view")
        .cursor_range = Some(cursor_range_from_char_range(0..10));

    app.open_search();

    assert_eq!(app.search_scope(), SearchScope::SelectionOnly);
    assert_eq!(
        app.search_scope_origin(),
        SearchScopeOrigin::SelectionDefault
    );

    app.set_search_query("alpha");
    wait_for_search_matches(&mut app, 1);
    assert_eq!(app.search_match_count(), 1);
}

#[test]
fn selection_only_scope_without_selection_reports_error_and_blocks_replace() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha beta".to_owned());

    app.open_search();
    app.set_search_scope(SearchScope::SelectionOnly);
    app.set_search_query("alpha");

    wait_for_search_condition(
        &mut app,
        |app| {
            !app.search_progress().searching
                && matches!(app.search_progress().status, SearchStatus::Error(_))
        },
        "selection-only error",
    );

    assert_eq!(app.search_match_count(), 0);
    match app.search_progress().status {
        SearchStatus::Error(message) => {
            assert_eq!(
                message,
                "Selection-only search requires an active selection."
            );
        }
        other => panic!("expected selection error, got {other:?}"),
    }
    assert_eq!(
        app.search_replace_availability(),
        SearchReplaceAvailability::Blocked(
            "Selection-only search requires an active selection.".to_owned(),
        )
    );
}

#[test]
fn invalid_regex_query_reports_invalid_status_and_blocks_replace() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha beta".to_owned());

    app.open_search();
    app.set_search_mode(SearchMode::Regex);
    app.set_search_query("[");

    wait_for_search_condition(
        &mut app,
        |app| {
            !app.search_progress().searching
                && matches!(app.search_progress().status, SearchStatus::InvalidQuery(_))
        },
        "invalid regex",
    );

    assert_eq!(app.search_match_count(), 0);
    match app.search_progress().status {
        SearchStatus::InvalidQuery(message) => {
            assert!(!message.is_empty());
        }
        other => panic!("expected invalid regex status, got {other:?}"),
    }
    assert!(matches!(
        app.search_replace_availability(),
        SearchReplaceAvailability::Blocked(_)
    ));
}

#[test]
fn submitting_search_request_keeps_existing_highlights_until_results_arrive() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha beta alpha".to_owned());
    app.open_search();
    app.tabs_mut()[0]
        .active_view_mut()
        .expect("active view")
        .search_highlights = SearchHighlightState {
        ranges: vec![0..5, 11..16],
        active_range_index: Some(0),
    };
    app.search_state.query = "alpha".to_owned();
    app.search_state.dirty = true;

    app.refresh_search_state();

    let highlights = &app.tabs()[0]
        .active_view()
        .expect("active view")
        .search_highlights;
    assert!(app.search_progress().searching);
    assert_eq!(highlights.ranges, vec![0..5, 11..16]);
    assert_eq!(highlights.active_range_index, Some(0));
}

#[test]
fn selection_only_replace_all_stays_within_the_selected_range() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha beta alpha".to_owned());
    app.tabs_mut()[0]
        .active_view_mut()
        .expect("active view")
        .cursor_range = Some(cursor_range_from_char_range(0..10));

    app.open_search();
    app.set_search_query("alpha");
    app.set_search_replacement("omega");

    wait_for_search_matches(&mut app, 1);
    assert!(app.replace_all_search_matches());
    assert_eq!(app.tabs()[0].active_buffer().text(), "omega beta alpha");
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
    assert!(app.replace_all_search_matches());
    assert_eq!(app.tabs()[0].active_buffer().text(), "omega beta omega");
    assert_eq!(app.tabs()[1].active_buffer().text(), "omega gamma");
}

#[test]
fn active_buffer_operation_undo_restores_search_replace_text() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha beta alpha".to_owned());

    app.open_search();
    app.set_search_query("alpha");
    app.set_search_replacement("omega");

    wait_for_search_matches(&mut app, 2);
    assert!(app.replace_current_search_match());
    assert_eq!(app.tabs()[0].active_buffer().text(), "omega beta alpha");

    app.handle_command(AppCommand::UndoActiveBufferTextOperation);
    assert_eq!(app.tabs()[0].active_buffer().text(), "alpha beta alpha");
}

#[test]
fn active_buffer_operation_redo_reapplies_search_replace_text() {
    let mut app = test_app();
    app.tabs_mut()[0]
        .buffer
        .replace_text("alpha beta alpha".to_owned());

    app.open_search();
    app.set_search_query("alpha");
    app.set_search_replacement("omega");

    wait_for_search_matches(&mut app, 2);
    assert!(app.replace_current_search_match());
    app.handle_command(AppCommand::UndoActiveBufferTextOperation);
    app.handle_command(AppCommand::RedoActiveBufferTextOperation);

    assert_eq!(app.tabs()[0].active_buffer().text(), "omega beta alpha");
}
