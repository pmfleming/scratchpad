use super::*;
use crate::app::{
    app_state::test_support::test_app,
    domain::{BufferState, WorkspaceTab},
};

#[test]
fn completed_snapshot_loads_cannot_replace_edits_replacements_or_closed_buffers() {
    for change in ["edit", "replace", "close", "cancel"] {
        let root = tempfile::tempdir().unwrap();
        let mut app = test_app(root.path());
        app.tab_manager.set_tabs(
            vec![WorkspaceTab::new(BufferState::new(
                "file".into(),
                "live".into(),
                None,
            ))],
            0,
        );
        let ctx = egui::Context::default();
        let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
        let pending = Pending {
            buffer_id: buffer.id,
            version_id: 1,
            expected: buffer.document().history_token(),
            source: source(&ctx, buffer),
            return_version: None,
            result: Arc::new(Mutex::new(Some(Ok(RestoredVersion {
                text: "obsolete snapshot".into(),
                history: Vec::new(),
                return_version: 2,
            })))),
        };
        ctx.data_mut(|d| d.insert_temp(pending_key(), pending));
        match change {
            "edit" => {
                app.tab_manager
                    .active_tab_mut()
                    .unwrap()
                    .active_buffer_mut()
                    .document_mut()
                    .insert_direct(0, "new ");
            }
            "replace" => {
                app.tab_manager
                    .active_tab_mut()
                    .unwrap()
                    .active_buffer_mut()
                    .install_history_document(TextDocument::new("replacement".into()));
            }
            "close" => {
                app.tab_manager.set_tabs(vec![WorkspaceTab::untitled()], 0);
            }
            "cancel" => cancel_pending(&ctx),
            _ => unreachable!(),
        }
        let before = app
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .document()
            .history_token();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| poll(ui, &mut app));
        output.textures_delta.clear();
        assert_eq!(
            app.tab_manager
                .active_tab()
                .unwrap()
                .active_buffer()
                .document()
                .history_token(),
            before,
            "{change}"
        );
        assert!(ctx.data(|d| d.get_temp::<Pending>(pending_key())).is_none());
    }
}

#[test]
fn reselecting_current_snapshot_cancels_an_older_pending_destination() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    let mut buffer = BufferState::new("file".into(), "selected".into(), None);
    buffer.set_snapshot_navigation(Some((1, 2)));
    let id = buffer.id;
    app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
    let ctx = egui::Context::default();
    let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
    let pending = Pending {
        buffer_id: id,
        version_id: 3,
        expected: buffer.document().history_token(),
        source: source(&ctx, buffer),
        return_version: Some(2),
        result: Arc::new(Mutex::new(None)),
    };
    ctx.data_mut(|d| d.insert_temp(pending_key(), pending));
    request(&ctx, &mut app, id, 1);
    assert!(ctx.data(|d| d.get_temp::<Pending>(pending_key())).is_none());
    assert_eq!(
        app.tab_manager.active_tab().unwrap().active_buffer().text(),
        "selected"
    );
}
