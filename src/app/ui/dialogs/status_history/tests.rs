use super::model::{Counts, StatusRow, clock_at, rows};
use crate::app::app_state::{
    ScratchpadApp, StatusDomain, StatusFilter, StatusMessage, StatusSeverity, StatusState,
    test_support::test_app,
};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};

fn message(id: u64, severity: StatusSeverity, text: &str) -> StatusMessage {
    StatusMessage {
        id,
        severity,
        domain: StatusDomain::Disk,
        text: text.to_owned(),
        detail: None,
        time: 0,
    }
}

fn harness(app: ScratchpadApp) -> Harness<'static, ScratchpadApp> {
    let mut harness = Harness::builder()
        .allow_missing_glyphs()
        .with_size(egui::vec2(1100.0, 900.0))
        .with_context_setup(|ctx| {
            crate::app::fonts::apply_editor_fonts(
                ctx,
                &crate::app::fonts::EditorFontSelection::scratchpad(
                    crate::app::fonts::EditorFontPreset::Standard,
                ),
                4.0,
            )
            .unwrap();
            ctx.options_mut(|o| o.warn_on_id_clash = true);
        })
        .build_ui_state(
            |ui, app| super::show_status_history_window(ui.ctx(), app),
            app,
        );
    harness.run_steps(3);
    harness
}

/// An app with three messages, oldest first, and the panel open.
fn app_with_messages(root: &std::path::Path) -> ScratchpadApp {
    let mut app = test_app(root);
    app.state.status = StatusState::default();
    let status = &mut app.state.status;
    status.set_info_status_in_domain(StatusDomain::File, "Saved notes.txt.");
    status.set_warning_status_in_domain(StatusDomain::Disk, "notes.txt changed on disk.");
    status.set_error_status_with_detail(
        StatusDomain::Settings,
        "Could not save settings.",
        "access denied",
    );
    app.state.dialogs.status_history.open();
    app
}

fn press(harness: &mut Harness<'static, ScratchpadApp>, key: egui::Key) {
    harness.key_press(key);
    harness.run_steps(2);
}

#[test]
fn rows_are_newest_first_and_fold_repeats() {
    let history = [
        message(1, StatusSeverity::Info, "Older"),
        message(2, StatusSeverity::Info, "Saved."),
        message(3, StatusSeverity::Info, "Saved."),
        message(4, StatusSeverity::Error, "Failed."),
    ];
    let listed = rows(history.iter(), StatusFilter::All);
    assert_eq!(
        listed
            .iter()
            .map(|row| (row.text.as_str(), row.repeats, row.id))
            .collect::<Vec<_>>(),
        vec![("Failed.", 1, 4), ("Saved.", 2, 3), ("Older", 1, 1)]
    );
    let errors = rows(history.iter(), StatusFilter::Errors);
    assert_eq!(errors.len(), 1);
    assert_eq!(Counts::of(history.iter()).get(StatusFilter::Info), 3);
}

#[test]
fn copied_text_keeps_detail_on_its_own_line() {
    let mut status = message(1, StatusSeverity::Error, "Could not save your session.");
    status.detail = Some("access denied".to_owned());
    let row = StatusRow::from_status(&status);
    assert_eq!(row.text, "Could not save your session.");
    assert_eq!(
        row.copy_text(),
        "Could not save your session.\naccess denied"
    );
}

#[test]
fn clock_shows_seconds_today_and_the_date_otherwise() {
    let now = chrono::Local::now();
    let today = now.timestamp() as u64;
    assert_eq!(clock_at(today, now), now.format("%H:%M:%S").to_string());
    assert!(clock_at(today - 3 * 86_400, now).contains(' '));
}

#[test]
fn opening_selects_the_newest_message_and_marks_errors_seen() {
    let root = tempfile::tempdir().unwrap();
    let app = app_with_messages(root.path());
    assert!(app.state.status.has_unseen_errors());
    let harness = harness(app);
    assert!(!harness.state().state.status.has_unseen_errors());
    // The footer shows the selected message in full, with its detail.
    harness.get_by_label("Could not save settings.");
    harness.get_by_label("access denied");
    harness.get_by_label("3 messages · 3 new");
    harness.get_by_label("Errors 1");
}

#[test]
fn arrows_move_the_selection_and_escape_closes() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = harness(app_with_messages(root.path()));
    press(&mut harness, egui::Key::ArrowDown);
    harness.get_by_label("notes.txt changed on disk.");
    assert!(harness.query_by_label("access denied").is_none());
    press(&mut harness, egui::Key::End);
    harness.get_by_label("Saved notes.txt.");
    press(&mut harness, egui::Key::Home);
    harness.get_by_label("Could not save settings.");
    press(&mut harness, egui::Key::Escape);
    assert!(!harness.state().state.dialogs.status_history.is_open());
}

#[test]
fn filters_narrow_the_list_and_clear_empties_it() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = harness(app_with_messages(root.path()));
    harness.get_by_label("Warnings 1").click();
    harness.run_steps(3);
    harness.get_by_label("1 message · 1 new");
    harness.get_by_label("notes.txt changed on disk.");
    harness.get_by_label("Clear").click();
    harness.run_steps(3);
    assert!(harness.state().state.status.history.is_empty());
    harness.get_by_label("No warnings");
}

#[test]
fn reopening_marks_only_later_messages_as_new() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = harness(app_with_messages(root.path()));
    press(&mut harness, egui::Key::Escape);
    let state = &mut harness.state_mut().state;
    state
        .status
        .set_info_status_in_domain(StatusDomain::File, "Saved again.");
    state.dialogs.status_history.open();
    harness.run_steps(3);
    harness.get_by_label("4 messages · 1 new");
}

#[test]
fn full_status_details_and_reset_action_are_visible_without_expanding() {
    use crate::app::services::session_manager;

    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    app.state.persistence.session_restore_failed = true;
    app.state
        .persistence
        .session_store
        .chunk_store_handle()
        .unwrap()
        .set_health(crate::app::services::chunk_store::Health::Unavailable(
            "injected format failure".into(),
        ));
    let message = "Could not restore your previous session. Start a new session to resume session saving. This complete message should wrap rather than being cut off.";
    let detail =
        "Session format version 999 is incompatible with this build (expected version 10).";
    app.state
        .status
        .set_error_status_with_detail(StatusDomain::Session, message, detail);
    app.state.dialogs.status_history.open();
    let mut harness = harness(app);
    assert!(harness.query_by_label(message).is_some());
    assert!(harness.query_by_label("Detail").is_none());
    assert!(harness.query_by_label("Start new session").is_some());
    // Choosing a filter cannot hide the failure reason above the reset action.
    harness.state_mut().state.dialogs.status_history.filter = StatusFilter::Info;
    harness.run_steps(2);
    assert!(harness.query_by_label(detail).is_some());
    harness.get_by_label("Start new session").click();
    harness.run_steps(2);
    assert!(harness.state().state.persistence.session_reset.is_some());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while harness.state().state.persistence.session_reset.is_some()
        && std::time::Instant::now() < deadline
    {
        session_manager::poll_session_reset(harness.state_mut());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!harness.state().state.persistence.session_restore_failed);
    let detail = harness
        .state()
        .state
        .status
        .current
        .as_ref()
        .unwrap()
        .detail
        .as_ref()
        .unwrap();
    let backup = detail
        .lines()
        .next()
        .unwrap()
        .strip_prefix("Previous session moved to: ")
        .unwrap();
    std::fs::remove_dir_all(backup).unwrap();
}
