use super::super::model::TextHistoryAction;
use super::{Source, source};
use crate::app::{
    app_state::{ScratchpadApp, StatusDomain, workspace::mutation},
    domain::{BufferState, TextDocument, buffer::DocumentRevision},
    services::session_store::{HistoryCheckpoint, RestoredVersion},
};
use eframe::egui;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
pub(super) struct Browse {
    pub source: Source,
    pub version_id: i64,
    pub return_version: i64,
    revision: DocumentRevision,
    history_revision: u64,
}

#[derive(Clone)]
struct Pending {
    buffer_id: u64,
    version_id: i64,
    expected: DocumentRevision,
    history_revision: u64,
    source: Source,
    return_version: Option<i64>,
    result: Arc<Mutex<Option<Result<RestoredVersion, String>>>>,
}

pub(super) fn browse_key(id: u64) -> egui::Id {
    egui::Id::unique(("history.snapshot.visit", id))
}
fn pending_key() -> egui::Id {
    egui::Id::unique("history.snapshot.pending")
}

pub(in super::super) fn cancel_pending(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<Pending>(pending_key()));
}

pub(in super::super) fn clear_history(ctx: &egui::Context, app: &mut ScratchpadApp) -> bool {
    cancel_pending(ctx);
    let mut changed = false;
    for buffer in app
        .tab_manager
        .tabs
        .as_mut_slice()
        .iter_mut()
        .flat_map(|tab| tab.buffers_mut())
    {
        if let Some(mut visit) = browse(ctx, buffer) {
            let mut source = (*visit.source.buffer).clone();
            source.document_mut().clear_operation_history();
            buffer.set_snapshot_navigation(None);
            visit.source.cancelled.store(true, Ordering::Relaxed);
            visit.source.cancelled = Arc::new(AtomicBool::new(false));
            visit.source.revision = source.document().revision_key();
            visit.source.history_revision = source.document().history_revision_counter();
            visit.source.buffer = Arc::new(source);
            ctx.data_mut(|data| {
                data.insert_temp(browse_key(buffer.id), visit);
            });
            changed = true;
        }
    }
    changed
}

pub(super) fn browse(ctx: &egui::Context, buffer: &BufferState) -> Option<Browse> {
    let mut visit = ctx.data(|data| data.get_temp::<Browse>(browse_key(buffer.id)))?;
    if visit.revision == buffer.document().revision_key()
        && visit.history_revision == buffer.document().history_revision_counter()
    {
        if visit.source.cancelled.load(Ordering::Relaxed) {
            visit.source.cancelled = Arc::new(AtomicBool::new(false));
            ctx.data_mut(|data| data.insert_temp(browse_key(buffer.id), visit.clone()));
        }
        Some(visit)
    } else {
        // Editing starts a new branch. The original text and native history were
        // durably checkpointed before navigation, not discarded here.
        visit.source.cancelled.store(true, Ordering::Relaxed);
        ctx.data_mut(|data| data.remove::<Browse>(browse_key(buffer.id)));
        None
    }
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
    let current = browse(ctx, buffer);
    if current
        .as_ref()
        .is_some_and(|visit| visit.version_id == version_id)
    {
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
        expected: buffer.document().revision_key(),
        history_revision: buffer.document().history_revision_counter(),
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
pub(in super::super) fn hydrate(ui: &mut egui::Ui, app: &ScratchpadApp) {
    for buffer in app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
    {
        let Some((version_id, return_version)) = buffer.snapshot_navigation() else {
            continue;
        };
        if browse(ui.ctx(), buffer).is_some() {
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
                Ok(Source {
                    revision: original.document().revision_key(),
                    history_revision: original.document().history_revision_counter(),
                    buffer: Arc::new(original),
                    cancelled: Arc::new(AtomicBool::new(false)),
                })
            },
        );
        match result {
            Some(Ok(source)) => {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        browse_key(buffer.id),
                        Browse {
                            source: source.as_ref().clone(),
                            version_id,
                            return_version,
                            revision: buffer.document().revision_key(),
                            history_revision: buffer.document().history_revision_counter(),
                        },
                    );
                });
            }
            Some(Err(error)) => {
                ui.weak("Newer history unavailable").on_hover_text(error);
            }
            None => {
                ui.weak("Loading newer history…");
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
        ui.weak("Loading snapshot…");
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
        .is_some_and(|buffer| {
            buffer.document().revision_key() == pending.expected
                && buffer.document().history_revision_counter() == pending.history_revision
        });
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
            let branch = !result.history.is_empty();
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
                ui.ctx()
                    .data_mut(|data| data.remove::<Browse>(browse_key(pending.buffer_id)));
                if !branch {
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
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(
                            browse_key(pending.buffer_id),
                            Browse {
                                source: pending.source,
                                version_id: pending.version_id,
                                return_version: pending
                                    .return_version
                                    .unwrap_or(result.return_version),
                                revision: buffer.document().revision_key(),
                                history_revision: buffer.document().history_revision_counter(),
                            },
                        )
                    });
                }
                super::super::navigation::select_buffer(ui.ctx(), pending.buffer_id);
                ui.ctx().request_repaint();
            }
        }
    }
}

pub(in super::super) fn return_for_edit(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    action: TextHistoryAction,
) -> Option<bool> {
    let buffer = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(action.buffer_id))?;
    let visit = browse(ctx, buffer)?;
    let mut document = visit.source.buffer.document().clone();
    let index = document
        .history_entries()
        .iter()
        .position(|entry| entry.id == action.entry_id)?;
    let result = if document.history_entries()[index].is_undone() {
        document
            .apply_text_history_redo(action.entry_id)
            .map(|_| ())
    } else {
        let later = document.history_entries()[index + 1..]
            .iter()
            .find(|entry| !entry.is_undone())
            .map(|entry| entry.id);
        later.map_or(Ok(()), |id| {
            document.apply_text_history_undo(id).map(|_| ())
        })
    };
    if result.is_err() {
        app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            "This history state is no longer replayable.",
        );
        return Some(false);
    }
    Some(install_return(ctx, app, action.buffer_id, document))
}

pub(in super::super) fn return_to_undo(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    id: u64,
) -> bool {
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))
    else {
        return false;
    };
    let Some(visit) = browse(ctx, buffer) else {
        return false;
    };
    let mut document = visit.source.buffer.document().clone();
    while document.undo_last_operation().is_some() {}
    install_return(ctx, app, id, document)
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
    ctx.data_mut(|data| data.remove::<Browse>(browse_key(id)));
    ctx.request_repaint();
    true
}
