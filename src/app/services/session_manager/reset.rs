use super::{Instant, ScratchpadApp, StatusDomain, capture_session, egui};
use crate::app::services::session_store::SessionStore;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(crate) struct PendingSessionReset {
    receiver: Receiver<(SessionStore, Result<PathBuf, String>)>,
}

pub(crate) fn start_new_session(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if !app.state.persistence.session_restore_failed
        || app.state.persistence.session_reset.is_some()
        || app.state.background_io.has_pending_background_actions()
    {
        return;
    }
    let request = capture_session(app);
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
        Ok(_) => app.state.persistence.session_reset = Some(PendingSessionReset { receiver }),
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
    app.state.persistence.session_reset = None;
    match result {
        Ok(backup) => {
            app.state.persistence.session_restore_failed = false;
            app.state.persistence.pending_closed_tabs.clear();
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
    app.state.status.set_error_status_with_detail(
        StatusDomain::Session,
        "Could not start a new session. Session saving is still paused. You can try again.",
        error,
    );
    app.state.dialogs.status_history.open();
}
