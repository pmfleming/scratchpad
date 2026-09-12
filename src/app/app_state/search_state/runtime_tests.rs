use super::*;
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
