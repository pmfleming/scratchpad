use super::*;
use crate::app::services::session_store::{CapturedClosedTab, SessionPersistRequest};

fn settle(h: &mut egui_kittest::Harness<'_, ScratchpadApp>) {
    for _ in 0..200 {
        h.run_steps(2);
        let state = shared(&h.ctx);
        let state = state.lock().unwrap();
        if state.pending.is_none() && state.fetched && state.reveal_now.is_none() {
            return;
        }
        drop(state);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("Timeline did not settle");
}

#[test]
fn latest_and_now_scroll_across_pages_without_navigating_or_selecting() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let mut other = BufferState::new("closed.txt".into(), "start".into(), None);
    for i in 0..220 {
        other
            .document_mut()
            .replace_exact_with_history(&format!("closed edit {i}"));
    }
    let mut save = SessionPersistRequest::capture(app.tab_manager.tabs.as_slice(), 0, 16.0, false);
    save.closed_tabs
        .push(CapturedClosedTab::capture(&WorkspaceTab::new(other), None));
    app.state
        .persistence
        .session_store
        .persist_request(save)
        .unwrap();
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Timeline").click();
    settle(&mut h);
    let before = text(h.state());
    let now = super::super::navigation::active_now(h.state()).unwrap().id;
    assert!(h.query_by_label("Latest").is_none());
    assert_eq!(h.query_all_by_label("Go to latest event").count(), 2);
    assert!(h.query_by_label("Now for note.txt").is_none());
    for position in 0..2 {
        h.get_by_label("Go to Now").click();
        settle(&mut h);
        h.run_steps(3);
        assert!(h.query_by_label("Now for note.txt").is_some(), "{}", {
            let shared = shared(&h.ctx);
            let state = shared.lock().unwrap();
            format!(
                "position={position}, offset={}, height={}, rows={}, now_index={:?}, pending={}, reveal={:?}, anchor={:?}, limit={}, error={:?}",
                state.offset,
                state.viewport_height,
                state.rows.len(),
                state
                    .rows
                    .iter()
                    .position(|r| matches!(r,Row::Entry(row) if row.id()==now)),
                state.pending.is_some(),
                state.reveal_now,
                state.anchor,
                state.limit,
                state.error
            )
        });
        assert!(h.query_by_label("Go to Now").is_none());
        assert_eq!(text(h.state()), before);
        assert_eq!(
            super::super::navigation::active_now(h.state()).unwrap().id,
            now
        );
        assert!(shared(&h.ctx).lock().unwrap().selected.is_none());
        let buttons: Vec<_> = h.query_all_by_label("Go to latest event").collect();
        assert!(buttons[0].rect().top() < buttons[1].rect().top());
        buttons[position].click();
        h.run_steps(5);
        assert_eq!(shared(&h.ctx).lock().unwrap().offset, 0.0);
        assert!(h.query_by_label("Go to Now").is_some());
        assert_eq!(text(h.state()), before);
        assert_eq!(
            super::super::navigation::active_now(h.state()).unwrap().id,
            now
        );
        assert!(shared(&h.ctx).lock().unwrap().selected.is_none());
    }
}

#[test]
fn now_tracks_document_navigation_file_switching_and_empty_root() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    let original = app.tab_manager.active_tab().unwrap().clone();
    let mut other = BufferState::new("other.txt".into(), "base".into(), None);
    other
        .document_mut()
        .replace_exact_with_history("other edit");
    app.tab_manager
        .set_tabs(vec![original, WorkspaceTab::new(other)], 0);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Timeline").click();
    settle(&mut h);
    let first = h.get_by_label("Now for note.txt").rect();
    assert!(h.query_by_label("Now for other.txt").is_none());
    let buffer = h
        .state()
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .id;
    let ctx = h.ctx.clone();
    assert!(super::super::super::navigate(
        &ctx,
        h.state_mut(),
        buffer,
        2
    ));
    h.run_steps(3);
    assert_ne!(h.get_by_label("Now for note.txt").rect(), first);
    assert!(super::super::super::navigate(
        &ctx,
        h.state_mut(),
        buffer,
        3
    ));
    h.run_steps(3);
    assert_eq!(h.get_by_label("Now for note.txt").rect(), first);
    h.state_mut().tab_manager.set_active_tab_index_clamped(1);
    h.run_steps(3);
    assert!(h.query_by_label("Now for note.txt").is_none());
    assert!(h.query_by_label("Now for other.txt").is_some());
    h.state_mut()
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut()
        .navigate_history(0)
        .unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Now for other.txt").is_none());
    assert!(h.query_by_label("Now: Empty document").is_some());
    assert!(h.query_by_label("Go to Now").is_none());
}
