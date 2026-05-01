use crate::app::app_state::ScratchpadApp;
use crate::app::services::session_store::{
    RestoreStatusLevel, RestoredSession, SessionPersistRequest,
};
use crate::app::services::settings_store::AppSettings;
use eframe::egui;
use std::time::Instant;

pub(crate) fn maybe_persist_session(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if !app.session_dirty() {
        return;
    }

    ctx.request_repaint_after(crate::app::app_state::SESSION_SNAPSHOT_INTERVAL);
    if app.last_session_persist.elapsed() < crate::app::app_state::SESSION_SNAPSHOT_INTERVAL {
        return;
    }
    if app.pending_background_actions.values().any(|action| {
        matches!(
            action,
            crate::app::app_state::PendingBackgroundAction::PersistSession(_)
        )
    }) {
        return;
    }

    let request = SessionPersistRequest::capture(
        app.tabs(),
        app.active_tab_index(),
        app.font_size(),
        app.word_wrap(),
    );
    app.clear_session_dirty();
    app.queue_background_session_persist(request);
}

pub(crate) fn persist_session_now(app: &mut ScratchpadApp) -> std::io::Result<()> {
    let request = SessionPersistRequest::capture(
        app.tabs(),
        app.active_tab_index(),
        app.font_size(),
        app.word_wrap(),
    );
    app.session_store.persist_request(request)?;
    app.clear_session_dirty();
    app.last_session_persist = Instant::now();
    Ok(())
}

pub(crate) fn restore_session_state(app: &mut ScratchpadApp) -> Option<AppSettings> {
    match app.session_store.load() {
        Ok(Some(restored)) => Some(apply_restored_session(app, restored)),
        Ok(None) => None,
        Err(error) => {
            app.set_error_status(format!("Session restore failed: {error}"));
            None
        }
    }
}

pub(crate) fn apply_restored_session(
    app: &mut ScratchpadApp,
    restored: RestoredSession,
) -> AppSettings {
    if let Some(status) = restored.restore_status.as_ref() {
        match status.level {
            RestoreStatusLevel::Info => app.set_info_status(status.message.clone()),
            RestoreStatusLevel::Warning => app.set_warning_status(status.message.clone()),
        }
    }
    app.tab_manager_mut().tabs = restored.tabs;
    app.tab_manager_mut().active_tab_index = restored.active_tab_index;
    app.ensure_active_tab_slot_selected();
    app.refresh_startup_restore_conflicts();
    restored.legacy_settings
}
