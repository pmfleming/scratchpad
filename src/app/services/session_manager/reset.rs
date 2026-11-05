use super::{Instant, ScratchpadApp, StatusDomain, capture_session, egui};
use crate::app::services::session_store::SessionStore;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(crate) struct PendingSessionReset {
    receiver: Receiver<(SessionStore, Result<PathBuf, String>)>,
    acknowledgement: crate::app::services::session_store::SessionCaptureAck,
}

pub(crate) fn start_new_session(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if !app.state.persistence.session_store.storage_unavailable()
        || app.state.persistence.session_reset.is_some()
        || app.state.persistence.close_in_flight
        || !app.state.persistence.close_queue.is_empty()
        || app.state.background_io.has_pending_background_actions()
        || app.tab_manager.tabs.as_slice().iter().any(|tab| {
            tab.buffers().any(|buffer| buffer.is_preparing_text())
                || tab
                    .layout
                    .views()
                    .iter()
                    .any(|view| !view.deferred_editor_events.is_empty())
        })
    {
        return;
    }
    app.state.persistence.session_store.set_bulk_stopped(true);
    let request = capture_session(app);
    let acknowledgement = request.acknowledgement();
    let mut store = app.state.persistence.session_store.clone();
    let (sender, receiver) = mpsc::channel();
    let ctx = ctx.clone();
    match std::thread::Builder::new()
        .name("session-reset".into())
        .spawn(move || {
            let result = store
                .start_new_session(request)
                .map_err(|error| error.to_string());
            let _ = sender.send((store, result));
            ctx.request_repaint();
        }) {
        Ok(_) => {
            app.state.persistence.session_reset = Some(PendingSessionReset {
                receiver,
                acknowledgement,
            });
        }
        Err(error) => report_reset_failure(app, error.to_string()),
    }
}

pub(crate) fn poll_session_reset(app: &mut ScratchpadApp) {
    let Some(pending) = &app.state.persistence.session_reset else {
        return;
    };
    let result = match pending.receiver.try_recv() {
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err("Session reset worker stopped unexpectedly.".into()),
        Ok((store, result)) => {
            app.state.persistence.session_store = store;
            result
        }
    };
    let acknowledgement = pending.acknowledgement.clone();
    app.state.persistence.session_reset = None;
    match result {
        Ok(backup) => {
            app.state.persistence.session_restore_failed = false;
            app.apply_history_budget_to_open_buffers();
            super::acknowledge_capture(app, &acknowledgement);
            app.state.persistence.last_session_persist = Instant::now();
            // Edits made while the reset was running must still be saved.
            app.tab_manager.mark_session_dirty();
            app.state.status.set_warning_status_with_detail(
                StatusDomain::Session,
                "New session created. Session saving has resumed.",
                format!(
                    "Previous session moved to: {}\nThis backup is temporary and may be deleted during temporary-file cleanup.",
                    backup.display()
                ),
            );
        }
        Err(error) => report_reset_failure(app, error),
    }
}

fn report_reset_failure(app: &mut ScratchpadApp, error: String) {
    app.state.persistence.session_store.set_bulk_stopped(false);
    app.state.status.set_error_status_with_detail(
        StatusDomain::Session,
        "Could not start a new session. Session saving is still paused. You can try again.",
        error,
    );
    app.state.dialogs.status_history.open();
}
