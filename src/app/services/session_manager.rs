use crate::app::app_state::workspace::restore_conflict;
use crate::app::app_state::{AppSurface, ScratchpadApp, StatusDomain};
use crate::app::services::session_store::{
    RestoreStatusLevel, RestoredSession, SessionActiveSurface, SessionPersistRequest,
};
use eframe::egui;
use std::time::Instant;

mod reset;
pub(crate) use reset::{PendingSessionReset, poll_session_reset, start_new_session};

pub(crate) fn maybe_persist_session(app: &mut ScratchpadApp, ctx: &egui::Context) {
    ctx.request_repaint_after(std::time::Duration::from_mins(1));
    if app.startup_restore_pending()
        || app.state.persistence.session_restore_failed
        || (!app.tab_manager.session_dirty
            && app.state.persistence.last_session_persist.elapsed()
                < std::time::Duration::from_mins(1))
    {
        return;
    }

    ctx.request_repaint_after(crate::app::app_state::SESSION_SNAPSHOT_INTERVAL);
    if app.state.persistence.last_session_persist.elapsed()
        < crate::app::app_state::SESSION_SNAPSHOT_INTERVAL
    {
        return;
    }
    if app.state.background_io.has_pending_persist() {
        return;
    }

    let request = capture_session(app);
    app.tab_manager.clear_session_dirty();
    app.queue_background_session_persist(request);
}

pub(crate) fn persist_session_now(app: &mut ScratchpadApp) -> std::io::Result<()> {
    // Covers explicit saves and Drop as well as exit. Never replace a committed
    // session with the startup placeholder, streamed prefix, or failed fallback.
    if app.startup_restore_pending() || app.state.persistence.session_restore_failed {
        return Ok(());
    }
    let request = capture_session(app);
    let acknowledgement = request.acknowledgement();
    app.state
        .persistence
        .session_store
        .persist_request(request)?;
    acknowledge_capture(app, &acknowledgement);
    app.tab_manager.clear_session_dirty();
    app.state.persistence.last_session_persist = Instant::now();
    Ok(())
}

/// Commit the closed archive and the remaining workspace before the UI removes
/// the tab. A failed transaction leaves the live tab and its latest edits intact.
pub(crate) fn persist_tab_close(app: &mut ScratchpadApp, index: usize) -> std::io::Result<()> {
    // A failed restore or permanently unavailable store must not trap the user.
    // Dirty-close confirmation still runs before these persistence helpers.
    if app.state.persistence.session_restore_failed
        || app.state.persistence.session_store.writes_blocked()
    {
        return Ok(());
    }
    if app.startup_restore_pending() {
        return Err(std::io::Error::other(
            "session recovery is not ready; tab was not closed",
        ));
    }
    let tab = app
        .tab_manager
        .tabs
        .as_slice()
        .get(index)
        .ok_or_else(|| std::io::Error::other("tab is no longer open"))?;
    let closed = crate::app::services::session_store::CapturedClosedTab::capture(
        tab,
        app.tab_manager.cold_session_tabs().get(&index),
    );
    let mut request = capture_session(app);
    request.close_tab(index, closed);
    let acknowledgement = request.acknowledgement();
    if let Err(error) = app.state.persistence.session_store.persist_request(request) {
        // The first explicit persistence attempt may discover an unavailable
        // lazy store. Reading health itself must never open or queue work.
        if app.state.persistence.session_store.writes_blocked() {
            app.state.status.report_session_save_failed(error);
            return Ok(());
        }
        return Err(error);
    }
    acknowledge_capture(app, &acknowledgement);
    Ok(())
}

fn capture_session(app: &ScratchpadApp) -> SessionPersistRequest {
    let mut request = SessionPersistRequest::capture_with_cold_tabs(
        app.tab_manager.tabs.as_slice(),
        app.tab_manager.cold_session_tabs(),
        app.tab_manager.active_tab_index,
        session_active_surface(app),
        app.state.app_settings.font_size(),
        app.state.app_settings.word_wrap(),
    );
    request
        .closed_tabs
        .clone_from(&app.state.persistence.pending_closed_tabs);
    request
        .activity
        .clone_from(&app.state.persistence.pending_activity);
    request.forgotten = app
        .state
        .persistence
        .pending_history_forgets
        .iter()
        .map(|(id, epoch)| (id.clone(), *epoch))
        .collect();
    request
}

pub(crate) fn acknowledge_capture(
    app: &mut ScratchpadApp,
    ack: &crate::app::services::session_store::SessionCaptureAck,
) {
    let sequences: std::collections::HashSet<_> = ack.activity_sequences.iter().copied().collect();
    app.state
        .persistence
        .pending_activity
        .retain(|event| !sequences.contains(&event.sequence));
    app.state
        .persistence
        .pending_closed_tabs
        .retain(|closed| !ack.closed_tokens.contains(&closed.token));
    app.state
        .persistence
        .pending_history_forgets
        .retain(|id, epoch| {
            !ack.forgotten
                .iter()
                .any(|(saved_id, saved_epoch)| saved_id == id && saved_epoch == epoch)
        });
}

/// Retain a final view's document and the remaining layout in the same commit.
pub(crate) fn persist_view_close(
    app: &mut ScratchpadApp,
    view: crate::app::domain::ViewId,
) -> std::io::Result<()> {
    // A failed restore or permanently unavailable store must not trap the user.
    // Dirty-close confirmation still runs before these persistence helpers.
    if app.state.persistence.session_restore_failed
        || app.state.persistence.session_store.writes_blocked()
    {
        return Ok(());
    }
    if app.startup_restore_pending() {
        return Err(std::io::Error::other(
            "session recovery is not ready; view was not closed",
        ));
    }
    let mut request = capture_session(app);
    if !request.close_view(
        app.tab_manager.tabs.as_slice(),
        app.tab_manager.active_tab_index,
        view,
    ) {
        return Err(std::io::Error::other("view is no longer open"));
    }
    let acknowledgement = request.acknowledgement();
    if let Err(error) = app.state.persistence.session_store.persist_request(request) {
        // The first explicit persistence attempt may discover an unavailable
        // lazy store. Reading health itself must never open or queue work.
        if app.state.persistence.session_store.writes_blocked() {
            app.state.status.report_session_save_failed(error);
            return Ok(());
        }
        return Err(error);
    }
    acknowledge_capture(app, &acknowledgement);
    Ok(())
}

pub(crate) fn restore_session_state(app: &mut ScratchpadApp) -> Option<()> {
    let result = app.state.persistence.session_store.load();
    report_database_notices(app);
    match result {
        Ok(Some(restored)) => {
            apply_restored_session(app, restored);
            Some(())
        }
        Ok(None) => None,
        Err(error) => {
            app.state.persistence.session_restore_failed = true;
            app.state.status.report_startup_restore_failed(error);
            app.state.dialogs.status_history.open();
            None
        }
    }
}

/// Database initialization can happen during restore, recovery queries or saving.
/// Notices are shared by store clones and consumed once into the [] status log.
pub(crate) fn report_database_notices(app: &mut ScratchpadApp) {
    for notice in app.state.persistence.session_store.take_notices() {
        match notice.level {
            RestoreStatusLevel::Info => app
                .state
                .status
                .set_info_status_in_domain(StatusDomain::Session, notice.message),
            RestoreStatusLevel::Warning => app
                .state
                .status
                .set_warning_status_in_domain(StatusDomain::Session, notice.message),
        }
    }
}

pub(crate) fn apply_restored_session(app: &mut ScratchpadApp, restored: RestoredSession) {
    if let Some(status) = restored.restore_status.as_ref() {
        match status.level {
            RestoreStatusLevel::Info => app
                .state
                .status
                .set_info_status_in_domain(StatusDomain::Session, status.message.clone()),
            RestoreStatusLevel::Warning => app
                .state
                .status
                .set_warning_status_in_domain(StatusDomain::Session, status.message.clone()),
        }
    }
    app.tab_manager
        .set_tabs(restored.tabs, restored.active_tab_index);
    app.tab_manager.evict_inactive_tab_state();
    apply_restored_active_surface(app, restored.active_surface);
    restore_conflict::refresh_startup_restore_conflicts(app);
}

pub(crate) fn session_active_surface(app: &ScratchpadApp) -> SessionActiveSurface {
    if crate::app::app_state::settings_state::showing_settings(app) {
        SessionActiveSurface::Settings
    } else {
        SessionActiveSurface::Workspace
    }
}

pub(crate) fn apply_restored_active_surface(
    app: &mut ScratchpadApp,
    active_surface: SessionActiveSurface,
) {
    let surface = match active_surface {
        SessionActiveSurface::Settings
            if crate::app::app_state::settings_state::settings_tab_open(app) =>
        {
            AppSurface::Settings
        }
        SessionActiveSurface::Settings | SessionActiveSurface::Workspace => AppSurface::Workspace,
    };
    app.state.chrome.set_active_surface(surface);
    crate::app::app_state::workspace::display_tabs::select_only_tab_slot(
        app,
        crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app),
    );
}
