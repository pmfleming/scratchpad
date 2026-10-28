use super::super::{self as snapshots, visit};
use crate::app::{
    app_state::{ScratchpadApp, test_support::test_app},
    domain::{BufferState, CursorRange, WorkspaceTab},
    fonts::{EditorFontPreset, EditorFontSelection, apply_editor_fonts},
    services::session_store::HistoryCheckpoint,
};
use eframe::{App, egui};
use egui_kittest::{Harness, kittest::Queryable};
use std::time::{Duration, Instant};

fn fixture(root: &std::path::Path) -> (ScratchpadApp, u64, i64, String) {
    let path = root.join("note.txt");
    std::fs::write(&path, "disk contents").unwrap();
    let mut app = test_app(root);
    let buffer = BufferState::new("note.txt".into(), "ancient".into(), Some(path));
    let id = buffer.id;
    let temp_id = buffer.temp_id.clone();
    app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
    crate::app::services::session_manager::persist_session_now(&mut app).unwrap();
    let point = app
        .state
        .persistence
        .session_store
        .buffer_versions(&temp_id)
        .unwrap()
        .remove(0);
    app.tab_manager.tabs.as_mut_slice()[0]
        .active_buffer_mut()
        .install_history_document(super::document());
    app.state.dialogs.text_history.open();
    (
        app,
        id,
        point.id,
        crate::app::ui::recovery::timestamp(point.created_at_unix_seconds),
    )
}

fn harness(app: ScratchpadApp) -> Harness<'static, ScratchpadApp> {
    Harness::builder()
        .with_size(egui::vec2(1200.0, 900.0))
        .with_context_setup(|ctx| {
            apply_editor_fonts(
                ctx,
                &EditorFontSelection::scratchpad(EditorFontPreset::Standard),
                4.0,
            )
            .unwrap();
        })
        .build_ui_state(
            |ui, app| App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
            app,
        )
}

fn wait(
    harness: &mut Harness<'_, ScratchpadApp>,
    done: impl Fn(&Harness<'_, ScratchpadApp>) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !done(harness) {
        assert!(
            Instant::now() < deadline,
            "history navigation did not complete"
        );
        std::thread::sleep(Duration::from_millis(2));
        harness.run_steps(1);
    }
}

fn text(harness: &Harness<'_, ScratchpadApp>) -> String {
    harness.state().tab_manager.tabs.as_slice()[0]
        .active_buffer()
        .text()
}

#[test]
fn snapshots_navigate_in_the_existing_editor_and_return_to_detailed_undo() {
    let root = tempfile::tempdir().unwrap();
    let (app, id, point, label) = fixture(root.path());
    let mut harness = harness(app);
    wait(&mut harness, |h| h.query_by_label(&label).is_some());
    harness.get_by_label(&label).click();
    wait(&mut harness, |h| text(h) == "ancient");
    assert_eq!(harness.state().tab_manager.tabs.len(), 1);
    let buffer = harness.state().tab_manager.tabs.as_slice()[0].active_buffer();
    assert_eq!(buffer.id, id);
    assert!(buffer.document().history_entries().is_empty());
    let return_id = visit::browse(&harness.ctx, buffer).unwrap().return_version;
    assert_ne!(return_id, point);
    assert!(harness.query_by_label("Recovery points").is_none());
    assert!(harness.query_by_label("History preview").is_none());
    assert!(harness.query_by_label("Recovery point").is_none());
    // Down first rejoins the oldest reconstructable state, then native redo.
    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(3);
    assert_eq!(text(&harness), "zero");
    for expected in ["one", "two"] {
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(3);
        assert_eq!(text(&harness), expected);
        assert_eq!(
            harness.state().tab_manager.tabs.as_slice()[0]
                .active_buffer()
                .document()
                .history_entries()
                .len(),
            2
        );
    }
    // A click on a normal row also returns from a snapshot to that exact state.
    let ctx = harness.ctx.clone();
    snapshots::request(&ctx, harness.state_mut(), id, point);
    wait(&mut harness, |h| text(h) == "ancient");
    let entries = snapshots::entries(&ctx, harness.state_mut());
    let target = entries.iter().max_by_key(|entry| entry.global_seq).unwrap();
    let action = crate::app::ui::dialogs::text_history::model::TextHistoryAction {
        buffer_id: id,
        entry_id: target.id,
    };
    assert!(crate::app::ui::dialogs::text_history::navigation::apply(
        &ctx,
        harness.state_mut(),
        action
    ));
    assert_eq!(text(&harness), "two");
    assert_eq!(
        std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
        "disk contents"
    );
}

#[test]
fn snapshot_steps_follow_the_selected_file_without_forcing_focus() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, id, point, _) = fixture(root.path());
    let original = app.tab_manager.tabs.as_slice()[0].clone();
    let other = WorkspaceTab::new(BufferState::new("other".into(), "untouched".into(), None));
    let other_id = other.active_buffer().id;
    app.tab_manager.set_tabs(vec![original, other], 1);
    let mut harness = harness(app);
    crate::app::ui::dialogs::text_history::write_follow_focus(&harness.ctx, false);
    harness.run_steps(2);
    assert!(!crate::app::ui::dialogs::text_history::read_follow_focus(
        &harness.ctx
    ));
    crate::app::ui::dialogs::text_history::navigation::select_buffer(&harness.ctx, id);
    for expected in ["one", "zero"] {
        harness.key_press(egui::Key::ArrowUp);
        harness.run_steps(3);
        assert_eq!(text(&harness), expected);
    }
    wait(&mut harness, |h| {
        h.ctx
            .data(|data| data.get_temp::<snapshots::FileSnapshots>(snapshots::catalog_key(id)))
            .is_some_and(|file| file.rows.iter().any(|row| row.point.id == point))
    });
    harness.key_press(egui::Key::ArrowUp);
    wait(&mut harness, |h| text(h) == "ancient");
    assert_eq!(
        harness
            .state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .id,
        other_id
    );
    assert_eq!(
        harness.state().tab_manager.tabs.as_slice()[1]
            .active_buffer()
            .text(),
        "untouched"
    );
}

#[test]
fn snapshot_position_and_native_return_history_survive_session_restart() {
    let root = tempfile::tempdir().unwrap();
    let (app, id, point, _) = fixture(root.path());
    let mut first = harness(app);
    let ctx = first.ctx.clone();
    snapshots::request(&ctx, first.state_mut(), id, point);
    wait(&mut first, |h| text(h) == "ancient");
    assert!(
        first.state().tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .snapshot_navigation()
            .is_some()
    );
    crate::app::services::session_manager::persist_session_now(first.state_mut()).unwrap();
    drop(first);
    drop(ctx);
    let mut restored = test_app(root.path());
    assert_eq!(
        restored.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .text(),
        "ancient"
    );
    assert!(
        restored.tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .snapshot_navigation()
            .is_some()
    );
    restored.state.dialogs.text_history.open();
    let mut second = harness(restored);
    wait(&mut second, |h| {
        visit::browse(
            &h.ctx,
            h.state().tab_manager.tabs.as_slice()[0].active_buffer(),
        )
        .is_some()
    });
    for expected in ["zero", "one", "two"] {
        second.key_press(egui::Key::ArrowDown);
        second.run_steps(3);
        assert_eq!(text(&second), expected);
    }
}

#[test]
fn covered_snapshot_is_a_marker_and_becomes_selectable_after_undo_expires() {
    let root = tempfile::tempdir().unwrap();
    let (mut app, _, _, label) = fixture(root.path());
    let buffer = app.tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
    buffer.replace_text("ancient".into());
    buffer
        .replace_char_ranges_with_undo(
            &[(0..0, "recent ".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    let mut harness = harness(app);
    let marker = format!("Snapshot · {label}");
    wait(&mut harness, |h| h.query_by_label(&marker).is_some());
    assert!(
        harness.query_by_label(&label).is_none(),
        "covered state must not have another selectable destination"
    );
    harness.state_mut().tab_manager.tabs.as_mut_slice()[0]
        .active_buffer_mut()
        .document_mut()
        .drop_oldest_history_entry()
        .unwrap();
    wait(&mut harness, |h| h.query_by_label(&label).is_some());
    harness.get_by_label(&label).click();
    wait(&mut harness, |h| text(h) == "ancient");
}

#[test]
fn editing_a_snapshot_preserves_the_previous_branch_and_stale_loads_do_not_replace_edits() {
    let root = tempfile::tempdir().unwrap();
    let (app, id, point, _) = fixture(root.path());
    let mut harness = harness(app);
    let ctx = harness.ctx.clone();
    snapshots::request(&ctx, harness.state_mut(), id, point);
    wait(&mut harness, |h| text(h) == "ancient");
    let buffer = harness.state().tab_manager.tabs.as_slice()[0].active_buffer();
    let return_id = visit::browse(&ctx, buffer).unwrap().return_version;
    let buffer = harness.state_mut().tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
    buffer
        .replace_char_ranges_with_undo(
            &[(0..0, "branch ".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    assert!(visit::browse(&ctx, buffer).is_none());
    let checkpoint = HistoryCheckpoint {
        temp_id: buffer.temp_id.clone(),
        snapshot: buffer.document_snapshot(),
        history: buffer.document().exported_history(),
    };
    let restored = harness
        .state()
        .state
        .persistence
        .session_store
        .navigate_version(checkpoint, return_id)
        .unwrap();
    assert_eq!(restored.text, "two");
    assert_eq!(restored.history.len(), 2);
    // Requesting a snapshot never gets to overwrite an edit made while loading.
    snapshots::request(&ctx, harness.state_mut(), id, point);
    harness.state_mut().tab_manager.tabs.as_mut_slice()[0]
        .active_buffer_mut()
        .replace_text("new edit".into());
    wait(&mut harness, |h| {
        h.state()
            .state
            .status
            .history
            .iter()
            .any(|status| status.text.contains("changed while the snapshot"))
    });
    assert_eq!(text(&harness), "new edit");
    assert_eq!(
        std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
        "disk contents"
    );
}
