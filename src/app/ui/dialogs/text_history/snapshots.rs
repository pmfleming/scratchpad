mod coverage;
mod render;
#[cfg(test)]
mod tests;
mod visit;

use super::model::{HistoryAction, TextHistoryFileGroup};
use crate::app::{
    app_state::ScratchpadApp,
    domain::{BufferState, buffer::DocumentRevision},
    text_history::{TextHistoryEntryView, entries_for_buffer},
};
use coverage::{SnapshotRow, classify_cancellable};
use eframe::egui;
pub(super) use render::{covered_markers, older_rows};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use visit::browse;
pub(super) use visit::{
    cancel_pending, clear_history, hydrate, poll, request, return_for_edit, return_to_undo,
};

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
    loading: bool,
    source_revision: Option<(DocumentRevision, u64)>,
    version_ids: Vec<i64>,
}
impl FileSnapshots {
    pub(super) fn older_change_count(&self) -> usize {
        self.rows.iter().filter(|row| row.covered.is_none()).count()
    }

    pub(super) fn has_snapshot_section(&self) -> bool {
        self.loading
            || self.error.is_some()
            || self.rows.iter().any(|row| {
                row.covered
                    .is_none_or(|boundary| boundary.entry_id.is_none())
            })
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

pub(super) fn entries(ctx: &egui::Context, app: &mut ScratchpadApp) -> Vec<TextHistoryEntryView> {
    let mut entries = crate::app::app_state::workspace::mutation::cached_text_history_entries(app);
    for buffer in app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
    {
        if let Some(visit) = browse(ctx, buffer) {
            entries.retain(|entry| entry.buffer_id != buffer.id);
            entries.extend(entries_for_buffer(&visit.source.buffer).into_iter().map(
                |mut entry| {
                    entry.undone = true;
                    entry
                },
            ));
        }
    }
    entries
}

/// Do not add every open buffer: only older changes outside the undo range
/// justify a group without edit rows. Loading/error/current-state markers do not.
pub(super) fn add_changed_snapshot_groups(
    catalog: &Catalog,
    groups: &mut Vec<TextHistoryFileGroup>,
) {
    let mut older: Vec<_> = catalog
        .iter()
        .filter(|(id, file)| {
            file.older_change_count() > 0 && !groups.iter().any(|group| group.buffer_id == **id)
        })
        .collect();
    older.sort_by(|(left_id, left), (right_id, right)| {
        left.label.cmp(&right.label).then(left_id.cmp(right_id))
    });
    groups.extend(
        older
            .into_iter()
            .map(|(&buffer_id, file)| TextHistoryFileGroup {
                buffer_id,
                label: file.label.clone(),
                rows: Vec::new(),
            }),
    );
}

pub(super) fn collect(ui: &egui::Ui, app: &ScratchpadApp) -> Catalog {
    ui.ctx()
        .data_mut(|data| data.insert_temp(running_key(), true));
    let active = app
        .tab_manager
        .active_tab()
        .map(|tab| tab.active_buffer().id);
    let selected = super::navigation::target_buffer(ui.ctx(), app);
    let mut catalog = Catalog::new();
    for buffer in app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
    {
        if Some(buffer.id) != active
            && Some(buffer.id) != selected
            && !super::list::file_expanded(ui.ctx(), buffer.id, active)
        {
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
            _ => file.loading = true,
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

pub(super) fn step(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    undo: bool,
) -> Option<HistoryAction> {
    let id = super::navigation::target_buffer(ctx, app)?;
    let buffer = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))?;
    let visit = browse(ctx, buffer);
    let stamp = visit.as_ref().map_or(
        (
            buffer.document().revision_key(),
            buffer.document().history_revision_counter(),
        ),
        |visit| (visit.source.revision, visit.source.history_revision),
    );
    let selected = visit.map(|visit| visit.version_id);
    if selected.is_none() {
        if let Some(action) = super::navigation::step_target(ctx, app, undo) {
            return Some(HistoryAction::Edit(action));
        }
        if !undo {
            return None;
        }
    }
    let file = ctx
        .data(|data| data.get_temp::<FileSnapshots>(catalog_key(id)))
        .unwrap_or_default();
    if file.source_revision != Some(stamp) {
        return (selected.is_some() && !undo).then_some(HistoryAction::Return { buffer_id: id });
    }
    let rows: Vec<_> = file
        .rows
        .iter()
        .filter(|row| row.covered.is_none())
        .collect();
    let index = selected.and_then(|id| rows.iter().position(|row| row.point.id == id));
    let next = match (selected, index, undo) {
        (None, _, true) => rows.first(),
        (Some(_), Some(index), true) => rows.get(index + 1),
        (Some(_), Some(index), false) if index > 0 => rows.get(index - 1),
        (Some(_), _, false) => return Some(HistoryAction::Return { buffer_id: id }),
        _ => None,
    }?;
    Some(HistoryAction::Snapshot {
        buffer_id: id,
        version_id: next.point.id,
    })
}

pub(super) fn execute(ctx: &egui::Context, app: &mut ScratchpadApp, action: HistoryAction) {
    match action {
        HistoryAction::Edit(action) => {
            super::navigation::apply(ctx, app, action);
        }
        HistoryAction::Snapshot {
            buffer_id,
            version_id,
        } => request(ctx, app, buffer_id, version_id),
        HistoryAction::Return { buffer_id } => {
            return_to_undo(ctx, app, buffer_id);
        }
    }
}
