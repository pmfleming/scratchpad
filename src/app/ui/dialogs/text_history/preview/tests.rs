use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn preview_restore_is_accessible_and_keeps_the_original_undoable() {
    let root = tempfile::tempdir().unwrap();
    let mut app = crate::app::app_state::test_support::test_app(root.path());
    app.tab_manager.set_tabs(
        vec![WorkspaceTab::new(BufferState::new(
            "note".into(),
            "current".into(),
            None,
        ))],
        0,
    );
    let id = app.tab_manager.active_tab().unwrap().active_buffer().id;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(900.0, 700.0))
        .with_context_setup(move |ctx| {
            open(ctx, id, "Earlier note".into(), Arc::new("earlier".into()));
        })
        .build_ui_state(|ui, app| show(ui.ctx(), app), app);
    harness.run_steps(3);
    harness
        .get_by_role_and_label(egui::Role::Button, "Restore into this tab")
        .click_accesskit();
    harness.run_steps(3);
    let buffer = harness
        .state_mut()
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    assert_eq!(buffer.text(), "earlier");
    buffer.document_mut().undo_last_operation().unwrap();
    assert_eq!(buffer.text(), "current");
}
