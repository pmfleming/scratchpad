mod coverage;
mod render;
#[cfg(test)]
mod tests;
mod visit;

use super::model::HistoryAction;
use crate::app::{
    app_state::ScratchpadApp,
    domain::{BufferState, buffer::DocumentRevision},
    text_history::{TextHistoryEntryView, entries_for_buffer},
};
use coverage::{SnapshotRow, classify_cancellable};
use eframe::egui;
pub(super) use render::{covered_positions, diamond, footer, older_rows};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use visit::browse;
pub(super) use visit::status_key;
pub(super) use visit::{cancel_pending, clear_history, hydrate, poll, request};

#[derive(Clone)]
struct Source {
    revision: DocumentRevision,
    history_revision: u64,
    buffer: Arc<BufferState>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
pub(super) struct FileSnapshots {
    pub(super) label: String,
    rows: Vec<SnapshotRow>,
    pub(super) selected: Option<i64>,
    error: Option<String>,
    source_revision: Option<(DocumentRevision, u64)>,
    version_ids: Vec<i64>,
}
impl FileSnapshots {
    pub(super) fn older_change_count(&self) -> usize {
        self.rows.iter().filter(|row| row.covered.is_none()).count()
    }
}
pub(super) type Catalog = HashMap<u64, FileSnapshots>;

#[derive(Clone)]
struct CoverageResult {
    revision: DocumentRevision,
    history_revision: u64,
    rows: Vec<SnapshotRow>,
    version_ids: Vec<i64>,
    cancelled: bool,
}

fn source_key(id: u64) -> egui::Id {
    egui::Id::unique(("history.snapshots.source", id))
}
fn tracked_key() -> egui::Id {
    egui::Id::unique("history.snapshots.tracked")
}
fn running_key() -> egui::Id {
    egui::Id::unique("history.snapshots.running")
}

pub(super) fn cleanup(ctx: &egui::Context, app: &ScratchpadApp) {
    ctx.data_mut(|data| {
        let mut removed = Vec::new();
        data.get_temp_mut_or_default::<HashSet<u64>>(tracked_key())
            .retain(|id| {
                let live = app
                    .tab_manager
                    .tabs
                    .as_slice()
                    .iter()
                    .any(|tab| tab.buffer_by_id(*id).is_some());
                if !live {
                    removed.push(*id);
                }
                live
            });
        for id in removed {
            if let Some(source) = data.get_temp::<Source>(source_key(id)) {
                source.cancelled.store(true, Ordering::Relaxed);
            }
            if let Some(visit) = data.get_temp::<visit::Browse>(visit::browse_key(id)) {
                visit.source.cancelled.store(true, Ordering::Relaxed);
            }
            data.remove::<Source>(source_key(id));
            data.remove::<visit::Browse>(visit::browse_key(id));
            data.remove::<FileSnapshots>(catalog_key(id));
        }
    });
}

fn source(ctx: &egui::Context, buffer: &BufferState) -> Source {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<HashSet<u64>>(tracked_key())
            .insert(buffer.id);
    });
    if let Some(visit) = browse(ctx, buffer) {
        return visit.source;
    }
    let key = source_key(buffer.id);
    let revision = buffer.document().revision_key();
    let history_revision = buffer.document().history_revision_counter();
    ctx.data_mut(|data| {
        if let Some(source) = data.get_temp::<Source>(key)
            && source.revision == revision
            && source.history_revision == history_revision
            && !source.cancelled.load(Ordering::Relaxed)
        {
            return source;
        }
        if let Some(previous) = data.get_temp::<Source>(key) {
            previous.cancelled.store(true, Ordering::Relaxed);
        }
        let source = Source {
            revision,
            history_revision,
            buffer: Arc::new(buffer.clone()),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        data.insert_temp(key, source.clone());
        source
    })
}

pub(super) fn preview_buffer(ctx: &egui::Context, buffer: &BufferState) -> BufferState {
    source(ctx, buffer).buffer.as_ref().clone()
}

pub(super) fn cancel_coverage(ctx: &egui::Context, app: &ScratchpadApp) {
    if ctx
        .data_mut(|data| data.remove_temp::<bool>(running_key()))
        .is_none()
    {
        return;
    }
    ctx.data(|data| {
        for buffer in app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(|tab| tab.buffers())
        {
            if let Some(source) = data.get_temp::<Source>(source_key(buffer.id)) {
                source.cancelled.store(true, Ordering::Relaxed);
            }
            if let Some(visit) = data.get_temp::<visit::Browse>(visit::browse_key(buffer.id)) {
                visit.source.cancelled.store(true, Ordering::Relaxed);
            }
        }
    });
}

pub(super) fn state_entries(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
) -> Arc<[TextHistoryEntryView]> {
    let entries = crate::app::app_state::workspace::mutation::cached_text_history_entries(app);
    let mut replaced = None;
    for buffer in app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
    {
        if let Some(visit) = browse(ctx, buffer) {
            let entries = replaced.get_or_insert_with(|| entries.to_vec());
            entries.retain(|entry| entry.buffer_id != buffer.id);
            let label = crate::app::text_history::buffer_label(buffer);
            entries.extend(entries_for_buffer(&visit.source.buffer).into_iter().map(
                |mut entry| {
                    entry.label.clone_from(&label);
                    entry
                },
            ));
        }
    }
    replaced.map_or(entries, Arc::from)
}

pub(super) fn collect(ui: &egui::Ui, app: &ScratchpadApp, buffers: &[u64]) -> Catalog {
    ui.ctx()
        .data_mut(|data| data.insert_temp(running_key(), true));
    let mut catalog = Catalog::new();
    for buffer in app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
    {
        if !buffers.contains(&buffer.id) {
            continue;
        }
        let source = source(ui.ctx(), buffer);
        let store = app.state.persistence.session_store.clone();
        let temp_id = buffer.temp_id.clone();
        let candidate = source.clone();
        let previous = ui
            .ctx()
            .data(|data| data.get_temp::<FileSnapshots>(catalog_key(buffer.id)));
        let result = crate::app::ui::recovery::query(
            ui,
            (
                "history.snapshots",
                store.root().to_owned(),
                temp_id.clone(),
            ),
            move || {
                let versions = store.buffer_versions(&temp_id)?;
                let version_ids: Vec<_> = versions.iter().map(|point| point.id).collect();
                let rows = match previous {
                    Some(previous)
                        if previous.source_revision
                            == Some((candidate.revision, candidate.history_revision))
                            && previous.version_ids == version_ids =>
                    {
                        previous.rows
                    }
                    _ => classify_cancellable(
                        candidate.buffer.document(),
                        versions,
                        &candidate.cancelled,
                    ),
                };
                Ok(CoverageResult {
                    revision: candidate.revision,
                    history_revision: candidate.history_revision,
                    rows,
                    version_ids,
                    cancelled: candidate.cancelled.load(Ordering::Relaxed),
                })
            },
        );
        let mut file = FileSnapshots {
            label: buffer.name.clone(),
            selected: browse(ui.ctx(), buffer).map(|visit| visit.version_id),
            ..Default::default()
        };
        match result {
            Some(Ok(result))
                if result.revision == source.revision
                    && result.history_revision == source.history_revision
                    && !result.cancelled =>
            {
                file.rows.clone_from(&result.rows);
                file.version_ids.clone_from(&result.version_ids);
                file.source_revision = Some((source.revision, source.history_revision));
            }
            Some(Err(error)) => file.error = Some(error),
            _ => {}
        }
        ui.ctx()
            .data_mut(|data| data.insert_temp(catalog_key(buffer.id), file.clone()));
        catalog.insert(buffer.id, file);
    }
    catalog
}

fn catalog_key(id: u64) -> egui::Id {
    egui::Id::unique(("history.snapshots.catalog", id))
}

pub(super) fn is_browsing(ctx: &egui::Context, app: &ScratchpadApp, id: u64) -> bool {
    app.tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))
        .is_some_and(|buffer| {
            browse(ctx, buffer).is_some() || buffer.snapshot_navigation().is_some()
        })
}

pub(super) fn go_to_state(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    id: u64,
    after: Option<u64>,
) -> bool {
    if !app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .any(|tab| tab.buffer_by_id(id).is_some())
    {
        return false;
    }
    cancel_pending(ctx);
    if let Some(result) = visit::return_to_state(ctx, app, id, after) {
        return result;
    }
    // Hydration must finish before a parked snapshot can target the native branch.
    if is_browsing(ctx, app, id) {
        return false;
    }
    let entries = state_entries(ctx, app);
    let states = super::states::States::new(&entries, id);
    let Some(position) = states.index(after) else {
        return false;
    };
    if !states.reachable(position) {
        return false;
    }
    match states.target(position) {
        Some(action) => super::navigation::apply(ctx, app, action),
        None => true,
    }
}

pub(super) fn execute(ctx: &egui::Context, app: &mut ScratchpadApp, action: HistoryAction) {
    match action {
        HistoryAction::PreviewSnapshot { .. } => super::preview::execute(ctx, app, action),
        HistoryAction::Preview { buffer_id, after } => {
            super::preview::select(ctx, app, buffer_id, after);
        }
        HistoryAction::State { buffer_id, after } => {
            go_to_state(ctx, app, buffer_id, after);
        }
        HistoryAction::Latest { buffer_id } => {
            visit::return_latest(ctx, app, buffer_id);
        }
        HistoryAction::Snapshot {
            buffer_id,
            version_id,
        } => request(ctx, app, buffer_id, version_id),
    }
}
