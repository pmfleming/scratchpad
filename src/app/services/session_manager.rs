use crate::app::app_state::{AppSurface, ScratchpadApp, StatusDomain};
use crate::app::services::session_store::{
    RestoreStatusLevel, RestoredSession, SessionActiveSurface, SessionPersistRequest,
};
use crate::app::services::settings_store::AppSettings;
use eframe::egui;
use std::time::Instant;

pub(crate) fn maybe_persist_session(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if !app.tab_manager.session_dirty {
        return;
    }

    ctx.request_repaint_after(crate::app::app_state::SESSION_SNAPSHOT_INTERVAL);
    if app.state.last_session_persist.elapsed() < crate::app::app_state::SESSION_SNAPSHOT_INTERVAL {
        return;
    }
    if app
        .state
        .io
        .pending_background_actions
        .values()
        .any(|action| {
            matches!(
                action,
                crate::app::app_state::PendingBackgroundAction::PersistSession(_)
            )
        })
    {
        return;
    }

    let request = SessionPersistRequest::capture_with_cold_tabs(
        app.tab_manager.tabs.as_slice(),
        app.tab_manager.cold_session_tabs(),
        app.tab_manager.active_tab_index,
        session_active_surface(app),
        app.state.app_settings.font_size(),
        app.state.app_settings.word_wrap(),
    );
    app.tab_manager.clear_session_dirty();
    app.queue_background_session_persist(request);
}

pub(crate) fn persist_session_now(app: &mut ScratchpadApp) -> std::io::Result<()> {
    let request = SessionPersistRequest::capture_with_cold_tabs(
        app.tab_manager.tabs.as_slice(),
        app.tab_manager.cold_session_tabs(),
        app.tab_manager.active_tab_index,
        session_active_surface(app),
        app.state.app_settings.font_size(),
        app.state.app_settings.word_wrap(),
    );
    app.state.session_store.persist_request(request)?;
    app.tab_manager.clear_session_dirty();
    app.state.last_session_persist = Instant::now();
    Ok(())
}

pub(crate) fn restore_session_state(app: &mut ScratchpadApp) -> Option<AppSettings> {
    match app.state.session_store.load() {
        Ok(Some(restored)) => Some(apply_restored_session(app, restored)),
        Ok(None) => None,
        Err(error) => {
            app.state.status.report_session_restore_failed(error);
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
    app.refresh_startup_restore_conflicts();
    restored.legacy_settings
}

pub(crate) fn session_active_surface(app: &ScratchpadApp) -> SessionActiveSurface {
    if app.showing_settings() {
        SessionActiveSurface::Settings
    } else {
        SessionActiveSurface::Workspace
    }
}

pub(crate) fn apply_restored_active_surface(
    app: &mut ScratchpadApp,
    active_surface: SessionActiveSurface,
) {
    app.state.active_surface = match active_surface {
        SessionActiveSurface::Settings if app.settings_tab_open() => AppSurface::Settings,
        SessionActiveSurface::Settings | SessionActiveSurface::Workspace => AppSurface::Workspace,
    };
    app.select_only_tab_slot(app.active_tab_slot_index());
}
