use super::*;
use crate::app::{
    domain::{
        BufferState, CharCursor, CursorRange, WorkspaceTab,
        buffer::{OperationRecord, TextDocumentEditOperation},
    },
    ui::dialogs::text_history::tests::{app, harness, text},
};
use egui_kittest::kittest::Queryable;

#[test]
fn live_index_moves_coalesced_edits_without_changing_row_identity_or_tracking_navigation() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    app.tab_manager.set_tabs(
        vec![WorkspaceTab::new(BufferState::new(
            "typing.txt".into(),
            String::new(),
            None,
        ))],
        0,
    );
    let mut index = model::LiveRows::default();
    let mut identity = None;
    let mut previous = 0;
    for (at, ch) in ["a", "b", "c"].into_iter().enumerate() {
        let doc = app
            .tab_manager
            .active_tab_mut()
            .unwrap()
            .active_buffer_mut()
            .document_mut();
        doc.insert_direct(at, ch);
        doc.push_edit_operation(OperationRecord {
            previous_cursor: CursorRange::one(CharCursor::new(at)),
            next_cursor: CursorRange::one(CharCursor::new(at + 1)),
            edits: vec![TextDocumentEditOperation {
                start_char: at,
                deleted_text: String::new(),
                inserted_text: ch.into(),
                deleted_spans: Vec::new(),
            }],
        });
        assert!(index.update(&app, "").1);
        assert_eq!(index.rows.len(), 1);
        let row = index.rows.values().next().unwrap();
        if let Some(id) = &identity {
            assert_eq!(row.id(), *id);
        } else {
            identity = Some(row.id());
        }
        assert!(row.cursor.sequence > previous);
        previous = row.cursor.sequence;
    }
    app.tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut()
        .navigate_history(0)
        .unwrap();
    assert!(!index.update(&app, "").1);
    assert_eq!(index.rows.len(), 1);
}

#[test]
fn timeline_keyboard_selects_across_files_and_first_typed_event_reaches_editor() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    let original = app.tab_manager.active_tab().unwrap().clone();
    let mut other = BufferState::new("other.txt".into(), "start".into(), None);
    other
        .document_mut()
        .replace_exact_with_history("other edited");
    app.tab_manager
        .set_tabs(vec![original, WorkspaceTab::new(other)], 0);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Timeline").click();
    h.run_steps(3);
    h.get_by_label("Replace with \"other edited\" · other.txt")
        .click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "other edited");
    h.key_press(egui::Key::End);
    h.run_steps(3);
    assert_eq!(text(h.state()), "one");
    assert_eq!(
        h.state().tab_manager.tabs.as_slice()[1]
            .active_buffer()
            .text(),
        "other edited"
    );
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(text(h.state()), "one");
    h.event(egui::Event::Text("X".into()));
    h.run_steps(3);
    assert!(text(h.state()).contains('X'));
}

#[test]
fn selecting_a_closed_edit_loads_one_document_then_seeks_without_writing_the_file() {
    use crate::app::services::{
        file_service::FileService,
        session_store::{CapturedClosedTab, SessionPersistRequest},
    };
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("closed.txt");
    std::fs::write(&path, "saved").unwrap();
    let mut app = app(root.path());
    let mut buffer = FileService::build_buffer_from_file_content(
        &path,
        FileService::read_file(&path).unwrap(),
        Some(FileService::read_disk_state(&path).unwrap()),
    );
    buffer.document_mut().replace_exact_with_history("unsaved");
    buffer.is_dirty = true;
    let id = buffer.temp_id.clone();
    let mut split = WorkspaceTab::new(buffer);
    split
        .open_buffer_as_split(
            BufferState::new("sibling".into(), "sibling text".into(), None),
            crate::app::domain::SplitAxis::Vertical,
            true,
            0.5,
        )
        .unwrap();
    let mut save = SessionPersistRequest::capture(app.tab_manager.tabs.as_slice(), 0, 16.0, false);
    save.closed_tabs
        .push(CapturedClosedTab::capture(&split, None));
    app.state
        .persistence
        .session_store
        .persist_request(save)
        .unwrap();
    std::fs::write(&path, "external").unwrap();
    apply(
        &egui::Context::default(),
        &mut app,
        Action {
            document: id.clone(),
            node: Some(2),
            inspect: false,
        },
    );
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(buffer.temp_id, id);
    assert_eq!(buffer.text(), "unsaved");
    assert_eq!(app.tab_manager.active_tab().unwrap().buffers().count(), 1);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "external");
    crate::app::services::session_manager::persist_session_now(&mut app).unwrap();
    assert!(
        app.state
            .persistence
            .session_store
            .closed_tabs()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn viewport_anchor_stays_on_its_row_when_new_activity_arrives() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let mut state = State::default();
    state.live.update(&app, "");
    state.rebuild();
    let row = state.live.rows.values().next().unwrap().clone();
    state.anchor = Some((row.id(), 7.0));
    let mut newest = row.clone();
    newest.cursor.sequence += 100;
    newest.cursor.node += 100;
    state.live.rows.insert(newest.cursor.clone(), newest);
    state.rebuild();
    let index = state
        .rows
        .iter()
        .position(|r| matches!(r,Row::Entry(r) if r.id()==row.id()))
        .unwrap();
    assert_eq!(state.offset, index as f32 * ROW_HEIGHT + 7.0);
}

#[test]
fn informational_rows_are_static_and_keyboard_selection_skips_them() {
    use crate::app::services::session_store::activity::{ActivityEvent, ActivityKind};
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    let event = ActivityEvent::new(
        ActivityKind::Opened,
        app.tab_manager.active_tab().unwrap().active_buffer(),
    );
    app.state.persistence.pending_activity.push(event);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Timeline").click();
    h.run_steps(3);
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Button, "Opened · note.txt")
            .is_none()
    );
    h.get_by_label("Opened · note.txt").hover();
    h.run_steps(3);
    assert_eq!(text(h.state()), "two");
    h.get_by_label("Opened · note.txt").click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "two");
    h.key_press(egui::Key::End);
    h.run_steps(3);
    assert_eq!(text(h.state()), "one");
    h.key_press(egui::Key::Home);
    h.run_steps(3);
    assert_eq!(text(h.state()), "two");
}

#[test]
fn closing_or_persisting_preserves_loaded_pages_and_the_reading_anchor() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    let ctx = egui::Context::default();
    let mut state = State::default();
    state.refresh(&app, "", &ctx);
    state.pending = None;
    state.fetched = true;
    state.next = None;
    state.limit = 1;
    let row = state.live.rows.values().next().unwrap().clone();
    state.anchor = Some((row.id(), 3.0));
    state.rebuild();
    app.tab_manager.set_tabs(Vec::new(), 0);
    state.refresh(&app, "", &ctx);
    assert!(state.persisted.values().any(|r| r.id() == row.id()));
    assert!(
        state
            .rows
            .iter()
            .any(|r| matches!(r,Row::Entry(r) if r.id()==row.id()))
    );
    assert!(state.offset >= 3.0);
    app.state.persistence.session_store.invalidate_history();
    state.refresh(&app, "", &ctx);
    assert!(state.persisted.is_empty());
    assert!(state.anchor.is_none());
}
