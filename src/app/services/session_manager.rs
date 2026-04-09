use crate::app::app_state::ScratchpadApp;
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

    if let Err(error) = persist_session_now(app) {
        app.set_error_status(format!("Session save failed: {error}"));
    }
}

pub(crate) fn persist_session_now(app: &mut ScratchpadApp) -> std::io::Result<()> {
    app.session_store.persist(
        app.tabs(),
        app.active_tab_index(),
        app.font_size,
        app.word_wrap,
        app.logging_enabled,
    )?;
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

fn apply_restored_session(
    app: &mut ScratchpadApp,
    restored: crate::app::services::session_store::RestoredSession,
) -> AppSettings {
    app.tab_manager_mut().tabs = restored.tabs;
    app.tab_manager_mut().active_tab_index = restored.active_tab_index;
    restored.legacy_settings
}
