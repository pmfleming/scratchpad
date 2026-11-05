//! Close intents keep their live tab until a background commit acknowledges the same revision.
use super::{ScratchpadApp, capture_session};
use crate::app::domain::{PaneNode, ViewId, WorkspaceTab, buffer::HistoryToken};
use crate::app::services::session_store::CapturedClosedTab;
#[derive(Clone)]
pub(crate) struct CloseAfterPersist {
    layout: u64,
    view: Option<ViewId>,
    root: PaneNode,
    revisions: Vec<(String, HistoryToken)>,
}
impl CloseAfterPersist {
    fn capture(tab: &WorkspaceTab, view: Option<ViewId>) -> Self {
        Self {
            layout: tab.layout_id,
            view,
            root: tab.layout.root_pane.clone(),
            revisions: tab
                .buffers()
                .map(|b| (b.temp_id.clone(), b.document().history_token()))
                .collect(),
        }
    }
    fn matches(&self, tab: &WorkspaceTab) -> bool {
        self.root == tab.layout.root_pane
            && self.revisions.len() == tab.buffers().count()
            && tab.buffers().all(|buffer| {
                !buffer.is_preparing_text()
                    && self.revisions.iter().any(|(id, revision)| {
                        *id == buffer.temp_id && *revision == buffer.document().history_token()
                    })
            })
            && tab
                .layout
                .views()
                .iter()
                .all(|v| v.deferred_editor_events.is_empty())
    }
}
pub(super) fn enqueue(
    app: &mut ScratchpadApp,
    index: usize,
    view: Option<ViewId>,
) -> std::io::Result<()> {
    let tab = app
        .tab_manager
        .tabs
        .as_slice()
        .get(index)
        .ok_or_else(|| std::io::Error::other("tab is no longer open"))?;
    let target = CloseAfterPersist::capture(tab, view);
    if !app
        .state
        .persistence
        .close_queue
        .iter()
        .any(|other| other.layout == target.layout && other.view == target.view)
    {
        app.state.persistence.close_queue.push_back(target);
    }
    poll_close_intents(app);
    Err(std::io::Error::new(
        std::io::ErrorKind::WouldBlock,
        "Close is waiting for its session commit",
    ))
}
pub(crate) fn poll_close_intents(app: &mut ScratchpadApp) {
    if app.state.persistence.close_in_flight || app.state.background_io.has_pending_persist() {
        return;
    }
    let Some(target) = app.state.persistence.close_queue.pop_front() else {
        return;
    };
    let Some(index) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .position(|t| t.layout_id == target.layout)
    else {
        return;
    };
    if !target.matches(&app.tab_manager.tabs.as_slice()[index]) {
        app.tab_manager.mark_session_dirty();
        return;
    }
    let mut request = capture_session(app);
    if let Some(view) = target.view {
        if !request.close_view(app.tab_manager.tabs.as_slice(), index, view) {
            return;
        }
    } else {
        let tab = &app.tab_manager.tabs.as_slice()[index];
        request.close_tab(
            index,
            CapturedClosedTab::capture(tab, app.tab_manager.cold_session_tabs().get(&index)),
        );
    }
    app.state.persistence.close_in_flight = true;
    app.queue_background_session_close(request, target);
}
pub(crate) fn finish_close(app: &mut ScratchpadApp, target: CloseAfterPersist) {
    let Some(index) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .position(|tab| tab.layout_id == target.layout)
    else {
        return;
    };
    if !target.matches(&app.tab_manager.tabs.as_slice()[index]) {
        app.tab_manager.mark_session_dirty();
        return;
    }
    if let Some(view) = target.view {
        crate::app::commands::workspace::perform_close_view_committed(app, view);
    } else {
        crate::app::app_state::workspace::lifecycle::close_tab_internal(app, index);
    }
}
pub(crate) fn finish_unavailable_close(app: &mut ScratchpadApp, target: CloseAfterPersist) {
    let Some(index) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .position(|tab| tab.layout_id == target.layout)
    else {
        return;
    };
    if !target.matches(&app.tab_manager.tabs.as_slice()[index]) {
        return;
    }
    retain_ram_close(app, index, target.view);
    finish_close(app, target);
}
pub(super) fn retain_ram_close(app: &mut ScratchpadApp, index: usize, view: Option<ViewId>) {
    let Some(tab) = app.tab_manager.tabs.as_slice().get(index) else {
        return;
    };
    if let Some(view) = view {
        if tab.is_last_view_for_buffer(view) != Some(true) {
            return;
        }
        if let Some(buffer) = tab.buffer_for_view(view) {
            app.state
                .persistence
                .pending_closed_tabs
                .push(CapturedClosedTab::capture(
                    &WorkspaceTab::new(buffer.clone()),
                    None,
                ));
        }
    } else {
        app.state
            .persistence
            .pending_closed_tabs
            .push(CapturedClosedTab::capture(
                tab,
                app.tab_manager.cold_session_tabs().get(&index),
            ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::app_state::{test_support, workspace::lifecycle};
    #[test]
    fn close_during_a_writer_pause_keeps_a_newer_edit_and_a_later_close_commits_it() {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_support::test_app(root.path());
        app.tab_manager.set_tabs(
            vec![WorkspaceTab::new(crate::app::domain::BufferState::new(
                "close".into(),
                "accepted".into(),
                None,
            ))],
            0,
        );
        crate::app::services::session_manager::persist_session_now(&mut app).unwrap();
        let store = app.state.persistence.session_store.clone();
        let pause = store
            .profile_pause_writer(std::time::Duration::from_millis(200))
            .unwrap();
        lifecycle::perform_close_tab(&mut app, 0);
        assert_eq!(app.tab_manager.tabs.len(), 1);
        app.tab_manager
            .active_tab_mut()
            .unwrap()
            .active_buffer_mut()
            .replace_text("newer accepted text".into());
        test_support::wait_for_background_io(&mut app);
        pause.join().unwrap().unwrap();
        assert_eq!(app.tab_manager.tabs.len(), 1);
        assert_eq!(
            app.tab_manager.active_tab().unwrap().active_buffer().text(),
            "newer accepted text"
        );
        lifecycle::perform_close_tab(&mut app, 0);
        test_support::wait_for_background_io(&mut app);
        assert!(
            app.tab_manager.tabs.is_empty(),
            "{:?}",
            app.state.status.current
        );
        let archive = store.closed_tabs().unwrap().remove(0);
        assert_eq!(
            store
                .restore_closed_tab(&archive.token)
                .unwrap()
                .active_buffer()
                .text(),
            "newer accepted text"
        );
    }
}
