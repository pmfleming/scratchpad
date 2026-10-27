use super::{
    ScratchpadApp,
    test_support::{logic, test_app},
};
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::instance_broker::{ElectionResult, LaunchRequest, PrimaryInstance};
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::SettingsStore;
use crate::app::startup::StartupOptions;
use eframe::{App, egui};
use std::time::{Duration, Instant};

mod restore;
mod retained;

fn hidden_input() -> egui::RawInput {
    let mut input = egui::RawInput {
        focused: false,
        ..Default::default()
    };
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.minimized = Some(true);
    viewport.focused = Some(false);
    input
}

#[test]
fn startup_restore_completes_without_a_single_ui_pass() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().join("session"));
    let tabs = vec![WorkspaceTab::new(BufferState::new(
        "restored.txt".to_owned(),
        "session text".to_owned(),
        None,
    ))];
    store.persist(&tabs, 0, 16.0, false).unwrap();
    let mut app = ScratchpadApp::with_stores_and_runtime_startup(
        store,
        SettingsStore::new(directory.path().join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    let ctx = egui::Context::default();
    let input = hidden_input();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.state.background_io.has_pending_background_actions() {
        assert!(
            Instant::now() < deadline,
            "hidden startup restore timed out"
        );
        let _ = logic(&mut app, &ctx, &input);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(ctx.cumulative_pass_nr(), 0);
    let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(buffer.name, "restored.txt");
    assert_eq!(buffer.text(), "session text");
}

#[test]
fn broker_activation_is_processed_while_minimized() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = test_app(directory.path());
    let ctx = egui::Context::default();
    let mut request = LaunchRequest {
        invocation_id: 1,
        sender_pid: std::process::id(),
        options: StartupOptions::default(),
        activate: true,
    };
    let state_root = directory.path().join("broker");
    let ElectionResult::Primary(primary) = PrimaryInstance::elect(&state_root, &request).unwrap()
    else {
        panic!("expected isolated primary");
    };
    app.attach_instance_broker(primary.start(&ctx).unwrap());
    request.invocation_id = 2;
    assert!(matches!(
        PrimaryInstance::elect(&state_root, &request).unwrap(),
        ElectionResult::Forwarded
    ));

    let output = logic(&mut app, &ctx, &hidden_input());
    let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
    assert!(commands.contains(&egui::ViewportCommand::Visible(true)));
    assert!(commands.contains(&egui::ViewportCommand::Minimized(false)));
    assert!(commands.contains(&egui::ViewportCommand::Focus));
    assert!(app.state.deferred_launch_requests.is_empty());
    assert_eq!(ctx.cumulative_pass_nr(), 0);
}

#[test]
fn minimized_close_is_handled_without_ui() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = test_app(directory.path());
    let ctx = egui::Context::default();
    let mut input = hidden_input();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let output = logic(&mut app, &ctx, &input);
    let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
    assert!(commands.contains(&egui::ViewportCommand::CancelClose));
    assert!(commands.contains(&egui::ViewportCommand::Close));
    assert!(app.state.window.close_in_progress);
    assert_eq!(ctx.cumulative_pass_nr(), 0);
}

#[test]
fn only_the_last_display_tab_exits_and_persists_an_empty_workspace() {
    for settings_open in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_app(directory.path());
        super::settings_controller::close_settings(&mut app);
        if settings_open {
            super::settings_controller::open_settings(&mut app);
        }
        app.tab_manager.create_untitled_tab();
        let ctx = egui::Context::default();
        super::workspace_controller::perform_close_tab(&mut app, 0);
        let _ = logic(&mut app, &ctx, &hidden_input());
        assert_eq!(app.tab_manager.tabs.len(), 1);
        assert!(!app.state.window.close_in_progress);
        super::workspace_controller::perform_close_tab(&mut app, 0);
        assert!(app.tab_manager.tabs.is_empty());
        if settings_open {
            let _ = logic(&mut app, &ctx, &hidden_input());
            assert!(
                !app.state.window.close_in_progress,
                "settings is still a display tab"
            );
            super::settings_controller::close_settings(&mut app);
        }
        let output = logic(&mut app, &ctx, &hidden_input());
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT]
                .contains(&egui::ViewportCommand::Close)
        );
        assert!(app.state.window.close_in_progress);
        assert!(
            app.state
                .persistence
                .session_store
                .load()
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn dirty_session_is_persisted_while_occluded() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = test_app(directory.path());
    app.tab_manager.session_dirty = true;
    app.state.persistence.last_session_persist =
        Instant::now().checked_sub(Duration::from_secs(2)).unwrap();
    let ctx = egui::Context::default();
    let mut input = hidden_input();
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.minimized = Some(false);
    viewport.occluded = Some(true);

    let _ = logic(&mut app, &ctx, &input);
    assert!(!app.tab_manager.session_dirty);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.state.background_io.has_pending_background_actions() {
        assert!(
            Instant::now() < deadline,
            "hidden session persistence timed out"
        );
        let _ = logic(&mut app, &ctx, &input);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        app.state
            .persistence
            .session_store
            .load()
            .unwrap()
            .is_some()
    );
    assert_eq!(ctx.cumulative_pass_nr(), 0);
}

#[test]
fn hidden_logic_does_not_replay_stale_dropped_files() {
    #[derive(Debug)]
    struct DroppedFile(std::path::PathBuf);
    impl egui::DroppedFile for DroppedFile {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let mut app = test_app(directory.path());
    let ctx = egui::Context::default();
    let path = directory.path().join("stale-drop.txt");
    std::fs::write(&path, "stale input").unwrap();
    let input = egui::RawInput {
        dropped_files: vec![std::sync::Arc::new(DroppedFile(path))],
        ..Default::default()
    };
    ctx.run_ui(input, |_| {}).drop_without_applying_deltas();
    assert_eq!(ctx.input(|i| i.raw.dropped_files.len()), 1);
    let _ = logic(&mut app, &ctx, &hidden_input());
    assert!(app.state.pending_open_file_paths.is_empty());
    assert!(!app.state.background_io.has_pending_background_actions());
}

#[test]
fn first_ui_pass_does_not_control_visibility_or_maximization() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = test_app(directory.path());
    let ctx = egui::Context::default();
    super::prepare_context_before_first_frame(&mut app, &ctx);
    let mut frame = eframe::Frame::_new_kittest();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        App::logic(&mut app, ui.ctx(), &mut frame);
        App::ui(&mut app, ui, &mut frame);
    });
    let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(!commands.iter().any(|command| matches!(
        command,
        egui::ViewportCommand::Visible(_) | egui::ViewportCommand::Maximized(_)
    )));
    output.drop_without_applying_deltas();
}
