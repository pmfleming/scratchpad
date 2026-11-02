use super::{Source, source};
use crate::app::{
    app_state::{ScratchpadApp, StatusDomain, workspace::mutation},
    domain::{TextDocument, buffer::HistoryToken},
    services::session_store::{HistoryCheckpoint, RestoredVersion},
};
use eframe::egui;
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod tests;

#[derive(Clone)]
struct Pending {
    buffer_id: u64,
    version_id: i64,
    expected: HistoryToken,
    source: Source,
    return_version: Option<i64>,
    result: Arc<Mutex<Option<Result<RestoredVersion, String>>>>,
}

fn pending_key() -> egui::Id {
    egui::Id::unique("history.snapshot.pending")
}

pub(in super::super) fn cancel_pending(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<Pending>(pending_key()));
}

/// Preserve a cleared return branch durably before changing either live history.
pub(in super::super) fn clear_history(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    buffer_ids: &[u64],
) -> Result<bool, String> {
    cancel_pending(ctx);
    let store = app.state.persistence.session_store.clone();
    let mut updates = Vec::new();
    for buffer in app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
        .filter(|buffer| buffer_ids.contains(&buffer.id))
    {
        let Some(visit) = buffer.snapshot_visit() else {
            continue;
        };
        let mut source = if let Some(source) = &visit.source {
            source.as_ref().clone()
        } else {
            let restored = store
                .read_history_version(buffer.temp_id.clone(), visit.return_version)
                .map_err(|e| e.to_string())?;
            let mut source = buffer.clone();
            source.install_history_document(TextDocument::with_preferred_line_ending(
                restored.text,
                buffer.format.preferred_line_ending_style(),
            ));
            source
        };
        source.document_mut().clear_operation_history();
        let checkpoint = HistoryCheckpoint {
            temp_id: buffer.temp_id.clone(),
            snapshot: source.document_snapshot(),
            history: Vec::new(),
        };
        let saved = store
            .navigate_version(checkpoint, visit.version_id)
            .map_err(|e| e.to_string())?;
        updates.push((
            buffer.id,
            visit.version_id,
            saved.return_version,
            Arc::new(source),
        ));
    }
    let changed = !updates.is_empty();
    for (id, version, back, source) in updates {
        let buffer = app
            .tab_manager
            .tabs
            .as_mut_slice()
            .iter_mut()
            .find_map(|tab| tab.buffer_by_id_mut(id))
            .unwrap();
        buffer.document_mut().clear_operation_history();
        buffer.set_snapshot_navigation(Some((version, back)));
        buffer.set_snapshot_return_buffer(source);
    }
    Ok(changed)
}

pub(in super::super) fn request(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    id: u64,
    version_id: i64,
) {
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))
    else {
        return;
    };
    if ctx
        .data(|data| data.get_temp::<Pending>(pending_key()))
        .is_some_and(|pending| {
            pending.buffer_id == id
                && pending.version_id == version_id
                && pending.expected == buffer.document().history_token()
        })
    {
        return;
    }
    let current = buffer.snapshot_visit();
    if current
        .as_ref()
        .is_some_and(|visit| visit.version_id == version_id)
    {
        cancel_pending(ctx);
        return;
    }
    if current.is_some_and(|visit| visit.source.is_none()) {
        set_status(ctx, "Loading newer history…".into());
        return;
    }
    let checkpoint = HistoryCheckpoint {
        temp_id: buffer.temp_id.clone(),
        snapshot: buffer.document_snapshot(),
        history: buffer.document().exported_history(),
    };
    let pending = Pending {
        buffer_id: id,
        version_id,
        expected: buffer.document().history_token(),

        source: source(ctx, buffer),
        return_version: current.map(|visit| visit.return_version),
        result: Arc::new(Mutex::new(None)),
    };
    let result = pending.result.clone();
    let store = app.state.persistence.session_store.clone();
    let repaint = ctx.clone();
    match std::thread::Builder::new()
        .name("history-snapshot".into())
        .spawn(move || {
            *result.lock().unwrap() = Some(
                store
                    .navigate_version(checkpoint, version_id)
                    .map_err(|error| error.to_string()),
            );
            repaint.request_repaint();
        }) {
        Ok(_) => {
            ctx.data_mut(|data| {
                data.insert_temp(pending_key(), pending);
            });
        }
        Err(error) => app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            format!("Could not load snapshot: {error}"),
        ),
    }
}

/// Reattach the native undo branch after restoring a session parked on a snapshot.
pub(in super::super) fn hydrate(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    for buffer in app
        .tab_manager
        .tabs
        .as_mut_slice()
        .iter_mut()
        .flat_map(|tab| tab.buffers_mut())
    {
        let Some((_, return_version)) = buffer.snapshot_navigation() else {
            continue;
        };
        if buffer
            .snapshot_visit()
            .is_some_and(|visit| visit.source.is_some())
        {
            continue;
        }
        let store = app.state.persistence.session_store.clone();
        let mut original = buffer.clone();
        let temp_id = buffer.temp_id.clone();
        let budget = app.state.app_settings.history.budget;
        let result = crate::app::ui::recovery::query(
            ui,
            (
                "history.return",
                store.root().to_owned(),
                temp_id.clone(),
                return_version,
            ),
            move || {
                let restored = store.read_history_version(temp_id, return_version)?;
                let mut document = TextDocument::with_preferred_line_ending(
                    restored.text,
                    original.format.preferred_line_ending_style(),
                );
                document.set_history_budget(budget);
                document.restore_exported_history(restored.history);
                original.install_history_document(document);
                Ok(original)
            },
        );
        match result {
            Some(Ok(source)) => {
                buffer.set_snapshot_return_buffer(source);
            }
            Some(Err(error)) => {
                set_status(ui.ctx(), format!("Newer history unavailable: {error}"));
            }
            None => {
                set_status(ui.ctx(), "Loading newer history…".into());
            }
        }
    }
}

pub(in super::super) fn poll(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let Some(pending) = ui
        .ctx()
        .data(|data| data.get_temp::<Pending>(pending_key()))
    else {
        return;
    };
    let Some(result) = pending.result.lock().unwrap().take() else {
        set_status(ui.ctx(), "Loading snapshot…".into());
        return;
    };
    ui.ctx()
        .data_mut(|data| data.remove::<Pending>(pending_key()));
    let unchanged = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(pending.buffer_id))
        .is_some_and(|buffer| buffer.document().history_token() == pending.expected);
    if !unchanged {
        app.state.status.set_warning_status_in_domain(
            StatusDomain::History,
            "The file changed while the snapshot was loading. Select it again.",
        );
        return;
    }
    match result {
        Err(error) => app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            format!("Could not load snapshot: {error}"),
        ),
        Ok(result) => {
            let mut document = TextDocument::with_preferred_line_ending(
                result.text,
                pending.source.buffer.format.preferred_line_ending_style(),
            );
            document.set_history_budget(app.state.app_settings.history.budget);
            document.restore_exported_history(result.history);
            if mutation::install_history_document(
                app,
                pending.buffer_id,
                document,
                super::super::read_follow_focus(ui.ctx()),
            ) {
                // Every snapshot, including a saved branch with its own undo,
                // retains a return route until the user edits or replays it.
                {
                    let buffer = app
                        .tab_manager
                        .tabs
                        .as_mut_slice()
                        .iter_mut()
                        .find_map(|tab| tab.buffer_by_id_mut(pending.buffer_id))
                        .unwrap();
                    buffer.set_snapshot_navigation(Some((
                        pending.version_id,
                        pending.return_version.unwrap_or(result.return_version),
                    )));
                    buffer.set_snapshot_return_buffer(pending.source.buffer);
                }
                super::super::navigation::retain_tool_focus(ui.ctx(), app);
                ui.ctx().request_repaint();
            }
        }
    }
}

pub(in super::super) fn status_key() -> egui::Id {
    egui::Id::unique("history.snapshot.status")
}

fn set_status(ctx: &egui::Context, status: String) {
    ctx.data_mut(|data| data.insert_temp(status_key(), status));
}

pub(in super::super) fn return_to_state(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    id: u64,
    after: Option<u64>,
) -> Option<bool> {
    let buffer = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))?;
    let source = buffer.snapshot_visit()?.source.as_ref()?;
    let entries = crate::app::text_history::entries_for_buffer(source);
    let states = super::super::states::States::new(&entries, id);
    let Some(position) = states.index(after) else {
        return Some(false);
    };
    if !states.reachable(position) {
        return Some(false);
    }
    let mut document = source.document().clone();
    if let Some(action) = states.target(position) {
        let result = if position < states.position {
            document.apply_text_history_undo(action.entry_id)
        } else {
            document.apply_text_history_redo(action.entry_id)
        };
        if result.is_err() {
            return Some(false);
        }
    }
    Some(install_return(ctx, app, id, document))
}

pub(in super::super) fn return_latest(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    id: u64,
) -> bool {
    cancel_pending(ctx);
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))
    else {
        return false;
    };
    let Some(source) = buffer
        .snapshot_visit()
        .and_then(|visit| visit.source.as_ref())
    else {
        return false;
    };
    // Return to the preserved live position, including any pre-existing redo tail.
    install_return(ctx, app, id, source.document().clone())
}

fn install_return(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    id: u64,
    document: TextDocument,
) -> bool {
    if !mutation::install_history_document(app, id, document, super::super::read_follow_focus(ctx))
    {
        return false;
    }
    super::super::navigation::retain_tool_focus(ctx, app);
    ctx.request_repaint();
    true
}
