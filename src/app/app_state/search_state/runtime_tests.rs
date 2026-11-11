use super::{ScratchpadApp, SearchScope, collect_search_targets, search_tab_label};
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::session_store::SessionStore;

#[test]
fn batched_search_labels_preserve_duplicate_names_and_active_first_order() {
    let directory = tempfile::tempdir().unwrap();
    let mut app =
        ScratchpadApp::with_session_store(SessionStore::new(directory.path().join("session")));
    app.set_session_persist_on_drop(false);
    let tabs = ["same.txt", "different.txt", "same.txt"]
        .into_iter()
        .map(|name| WorkspaceTab::new(BufferState::new(name.into(), "needle".into(), None)))
        .collect();
    app.tab_manager.set_tabs(tabs, 2);
    let targets = collect_search_targets(&app, SearchScope::AllOpenTabs);
    assert_eq!(
        targets
            .iter()
            .map(|target| target.tab_index)
            .collect::<Vec<_>>(),
        vec![2, 0, 1]
    );
    for target in targets {
        assert_eq!(target.tab_label, search_tab_label(&app, target.tab_index));
    }

    // Counts are request-local: renaming a tab must not leave stale duplicate labels.
    app.tab_manager.tabs.as_mut_slice()[2].buffers.buffer.name = "renamed.txt".into();
    for target in collect_search_targets(&app, SearchScope::AllOpenTabs) {
        assert_eq!(target.tab_label, search_tab_label(&app, target.tab_index));
    }
}

#[test]
fn result_bursts_keep_the_terminal_result_and_ignore_stale_generations() {
    use super::{SearchResult, SearchStatus, poll_search_results};
    let root = tempfile::tempdir().unwrap();
    let mut app = crate::app::app_state::test_support::test_app(root.path());
    let (tx, rx) = std::sync::mpsc::channel();
    app.state.search_state.runtime.result_rx = rx;
    app.state.search_state.runtime.requested_generation = 2;
    for generation in std::iter::repeat_n(1, 130).chain([2]) {
        tx.send(SearchResult {
            generation,
            matches: Vec::new(),
            displayed_match_count: 0,
            result_groups: Vec::new(),
            status: SearchStatus::Ready,
        })
        .unwrap();
    }
    poll_search_results(&mut app);
    assert_ne!(app.state.search_state.runtime.applied_generation, 2);
    poll_search_results(&mut app);
    assert_ne!(app.state.search_state.runtime.applied_generation, 2);
    poll_search_results(&mut app);
    assert_eq!(app.state.search_state.runtime.applied_generation, 2);
    assert!(matches!(
        app.state.search_state.runtime.status,
        SearchStatus::Ready
    ));
}
