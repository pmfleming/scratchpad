//! Prepare against snapshots, preflight every target, then install together.
//! A queued edit is never treated as a completed replacement.
use super::super::helpers::{fallback_selection_for_target, next_selection_for_target};
use super::super::{ReplacementPlan, ScratchpadApp, runtime};
use crate::app::{
    app_state::StatusDomain,
    domain::{
        CursorRevealMode, PieceSource,
        buffer::{PrepareTextRequest, PreparedText},
    },
};
use std::sync::mpsc::{self, Receiver, TryRecvError};

type PreparedTargets = Result<Vec<(u64, PreparedText)>, String>;
pub(crate) struct PendingReplace {
    pub(crate) plan: ReplacementPlan,
    rx: Receiver<PreparedTargets>,
    all: bool,
    bases: Vec<(u64, crate::app::domain::buffer::HistoryToken)>,
}

pub(super) fn start(app: &mut ScratchpadApp, plan: ReplacementPlan) -> bool {
    start_kind(app, plan, true)
}
pub(super) fn start_single(app: &mut ScratchpadApp, plan: ReplacementPlan) -> bool {
    start_kind(app, plan, false)
}
fn start_kind(app: &mut ScratchpadApp, plan: ReplacementPlan, all: bool) -> bool {
    app.state.search_state.replace.last_operation = None;
    if !plan
        .targets
        .iter()
        .all(|t| super::replace_all::validate_replacement_target(app, t))
    {
        app.state.status.report_search_results_stale_for_replace();
        fail(app, "Search results changed. No files were changed.");
        return false;
    }
    let mut bases = Vec::new();
    let mut requests: Vec<(u64, PrepareTextRequest)> = Vec::new();
    for target in &plan.targets {
        let Some(tab) = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .find(|t| t.buffer_by_id(target.buffer_id).is_some())
        else {
            return false;
        };
        let Some(buffer) = tab.buffer_by_id(target.buffer_id) else {
            return false;
        };
        bases.push((target.buffer_id, buffer.document().history_token()));
        let mut snapshot = buffer.clone();
        let previous = tab
            .layout
            .view(target.view_id)
            .and_then(|v| v.cursor_range)
            .unwrap_or_else(|| fallback_selection_for_target(target));
        if !snapshot.queue_text_preparation(
            target.replacements.clone(),
            previous,
            Some(next_selection_for_target(target)),
            PieceSource::SearchReplace,
            Some(target.view_id),
        ) {
            fail(app, "Could not prepare replacement. No files were changed.");
            return false;
        }
        let Some(request) = snapshot.take_text_preparation() else {
            return false;
        };
        requests.push((target.buffer_id, request));
    }
    let (tx, rx) = mpsc::channel();
    let store = app.state.persistence.session_store.opened_chunk_store();
    let repaint = app.state.search_state.runtime.repaint.clone();
    // Small operations complete synchronously, still using the same prepare /
    // preflight / install contract. Large operations stay off the UI thread.
    let bulk = is_bulk_plan(&plan);
    let prepare = move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            requests
                .into_iter()
                .map(|(id, request)| {
                    request
                        .prepare_with_store(store.clone())
                        .map(|p| (id, p))
                        .map_err(|e| e.to_string())
                })
                .collect::<PreparedTargets>()
        }))
        .unwrap_or_else(|_| Err("Text became unreadable while preparing replacement".into()));
        let _ = tx.send(result);
        repaint.notify();
    };
    app.state.search_state.replace.outcome = Some(format!(
        "Preparing {} matches in {} files…",
        plan.total_match_count,
        plan.targets.len()
    ));
    app.state.search_state.replace.pending = Some(PendingReplace {
        plan,
        rx,
        all,
        bases,
    });
    if bulk {
        if let Err(error) = std::thread::Builder::new()
            .name("search-replace".into())
            .spawn(prepare)
        {
            fail(
                app,
                &format!("Could not start replacement: {error}. No files were changed."),
            );
            return false;
        }
        true
    } else {
        prepare();
        poll(app);
        app.state
            .search_state
            .replace
            .outcome
            .as_ref()
            .is_some_and(|m| m.starts_with("Replaced "))
    }
}

pub(crate) fn poll(app: &mut ScratchpadApp) {
    let Some(pending) = app.state.search_state.replace.pending.as_ref() else {
        return;
    };
    let result = match pending.rx.try_recv() {
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err("Replacement preparation stopped".into()),
        Ok(result) => result,
    };
    let Some(pending) = app.state.search_state.replace.pending.take() else {
        return;
    };
    let all = pending.all;
    let prepared = match result {
        Ok(p) => p,
        Err(error) => {
            fail(app, &format!("{error}. No files were changed."));
            return;
        }
    };
    if !pending
        .plan
        .targets
        .iter()
        .all(|t| super::replace_all::validate_replacement_target(app, t))
        || !pending.bases.iter().all(|(id, token)| {
            app.tab_manager
                .tabs
                .as_slice()
                .iter()
                .find_map(|t| t.buffer_by_id(*id))
                .is_some_and(|b| b.document().history_token() == *token)
        })
    {
        fail(
            app,
            "A file changed while preparing replacement. No files were changed.",
        );
        return;
    }
    let operation = crate::app::domain::buffer::next_text_history_global_seq();
    let (staged, locked_updated) = match stage_targets(app, prepared, all, operation) {
        Ok(staged) => staged,
        Err(message) => {
            fail(app, message);
            return;
        }
    };
    publish(app, pending, operation, staged, locked_updated);
}

fn publish(
    app: &mut ScratchpadApp,
    pending: PendingReplace,
    operation: u64,
    staged: Vec<StagedTarget>,
    locked_updated: Option<super::super::selection::LockedSelection>,
) {
    let all = pending.all;
    let mut event = None;
    for StagedTarget {
        index,
        id,
        buffer,
        selection,
        views,
    } in staged
    {
        if all && event.is_none() {
            let e = crate::app::services::session_store::activity::ActivityEvent::new(
                crate::app::services::session_store::activity::ActivityKind::ReplaceAll {
                    operation_id: operation,
                    files: pending.plan.targets.len(),
                    matches: pending.plan.total_match_count,
                },
                &buffer,
            );
            event = Some(e);
        }
        let tab = &mut app.tab_manager.tabs.as_mut_slice()[index];
        // The entire target set was checked before publication, and this loop
        // never removes a buffer or yields to another event.
        if let Some(destination) = tab.buffer_by_id_mut(id) {
            *destination = buffer;
        }
        for (view_id, transformed) in views {
            if let Some((buffer, view)) = tab.buffer_and_view_mut(view_id) {
                view.layout_cache.clear();
                view.set_cursor_range_anchored(buffer, transformed);
                view.set_pending_cursor_range_anchored(buffer, transformed);
                view.request_cursor_reveal(CursorRevealMode::KeepVisible);
            }
        }
        if !all {
            app.state.search_state.replace.navigate_after =
                Some((id, selection.sorted_indices().1));
        }
        super::finalize_tab_buffer_mutation(app, index, id);
    }
    if let Some(locked) = locked_updated {
        app.state.search_state.query.selection = Some(locked);
    }
    if let Some(event) = event {
        app.state.persistence.pending_activity.push(event);
    }
    let message = format!(
        "Replaced {} matches in {} files.",
        pending.plan.total_match_count,
        pending.plan.targets.len()
    );
    app.state
        .status
        .set_info_status_in_domain(StatusDomain::Search, message.clone());
    app.state.search_state.replace.outcome = Some(message);
    if all {
        app.state.search_state.replace.last_operation =
            Some((operation, pending.plan.targets.len()));
    }
    app.state
        .search_state
        .replace
        .pending_replace_all_confirmation = None;
    app.tab_manager.mark_session_dirty();
    runtime::mark_search_dirty(app);
}
fn fail(app: &mut ScratchpadApp, message: &str) {
    app.state.search_state.replace.last_operation = None;
    app.state.search_state.replace.pending = None;
    app.state.search_state.replace.outcome = Some(message.into());
    app.state
        .status
        .set_error_status_in_domain(StatusDomain::Search, message);
    runtime::mark_search_dirty(app);
}

pub(crate) fn can_undo(app: &ScratchpadApp, operation: u64, files: usize) -> bool {
    let buffers: Vec<_> = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|t| t.buffers())
        .filter(|b| {
            b.document()
                .history_entries()
                .iter()
                .any(|e| e.global_seq == operation && e.source == PieceSource::SearchReplace)
        })
        .collect();
    buffers.len() == files
        && buffers.iter().all(|b| {
            !b.is_preparing_text()
                && b.document()
                    .history_entries()
                    .iter()
                    .any(|e| e.global_seq == operation && b.document().history_now() == e.id)
        })
}
pub(crate) fn undo(app: &mut ScratchpadApp, operation: u64, files: usize) -> bool {
    if !can_undo(app, operation, files) {
        return false;
    }
    let mut staged = Vec::new();
    for (index, tab) in app.tab_manager.tabs.as_slice().iter().enumerate() {
        for b in tab.buffers().filter(|b| {
            b.document()
                .history_entries()
                .iter()
                .any(|e| e.global_seq == operation && e.source == PieceSource::SearchReplace)
        }) {
            let mut next = b.clone();
            let Some(selection) = next.undo_last_text_operation() else {
                return false;
            };
            staged.push((index, b.id, next, selection));
        }
    }
    for (index, id, buffer, selection) in staged {
        let tab = &mut app.tab_manager.tabs.as_mut_slice()[index];
        // The entire target set was checked before publication, and this loop
        // never removes a buffer or yields to another event.
        if let Some(destination) = tab.buffer_by_id_mut(id) {
            *destination = buffer;
        }
        let views: Vec<_> = tab
            .layout
            .views()
            .iter()
            .filter(|v| v.buffer_id == id)
            .map(|v| v.id)
            .collect();
        for id in views {
            if let Some((buffer, view)) = tab.buffer_and_view_mut(id) {
                view.layout_cache.clear();
                view.set_cursor_range_anchored(buffer, selection);
                view.set_pending_cursor_range_anchored(buffer, selection);
            }
        }
        super::finalize_tab_buffer_mutation(app, index, id);
    }
    app.state.search_state.replace.outcome = Some("Undid replacement in every file.".into());
    app.tab_manager.mark_session_dirty();
    runtime::mark_search_dirty(app);
    true
}

struct StagedTarget {
    index: usize,
    id: u64,
    buffer: crate::app::domain::BufferState,
    selection: crate::app::domain::CursorRange,
    views: Vec<(u64, crate::app::domain::CursorRange)>,
}
fn stage_targets(
    app: &ScratchpadApp,
    prepared: Vec<(u64, PreparedText)>,
    all: bool,
    operation: u64,
) -> Result<
    (
        Vec<StagedTarget>,
        Option<super::super::selection::LockedSelection>,
    ),
    &'static str,
> {
    // Install into clones first: even a rejected installation cannot expose a
    // partial operation. Original buffers and selections are untouched so far.
    let locked = super::super::selection::resolve(app);
    let mut locked_updated = None;
    let mut staged = Vec::new();
    for (id, prepared) in prepared {
        let Some((index, buffer)) = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .enumerate()
            .find_map(|(i, t)| t.buffer_by_id(id).map(|b| (i, b)))
        else {
            return Err("A replacement target closed. No files were changed.");
        };
        let tab = &app.tab_manager.tabs.as_slice()[index];
        let views: Vec<_> = tab
            .layout
            .views()
            .iter()
            .filter(|v| v.buffer_id == id)
            .map(|v| {
                (
                    v.id,
                    prepared.transformed_selection(v.id, v.cursor_range.or(v.pending_cursor_range)),
                )
            })
            .collect();
        let locked_range = locked
            .as_ref()
            .filter(|(_, b, _, _)| *b == id)
            .map(|(_, _, view, range)| (*view, prepared.transformed_range(range.clone())));
        let mut next = buffer.clone();
        // The request was made from a clone with its own document identity;
        // install with the live revision captured by the plan after preflight.
        let prepared = prepared.rebase_after_preflight(next.document().history_token());
        let Ok(selection) = next.install_prepared_text(prepared) else {
            return Err("Could not install replacement. No files were changed.");
        };
        if all {
            next.document_mut().stamp_search_operation(operation);
        }
        if let Some((view, range)) = locked_range {
            let (start, end) = (range.start, range.end);
            let tree = next.document_mut().piece_tree_mut();
            locked_updated = Some(super::super::selection::LockedSelection {
                buffer: id,
                view,
                start: tree.create_anchor(start, crate::app::domain::buffer::AnchorBias::Left),
                end: tree.create_anchor(end, crate::app::domain::buffer::AnchorBias::Right),
            });
        }
        staged.push(StagedTarget {
            index,
            id,
            buffer: next,
            selection,
            views,
        });
    }
    Ok((staged, locked_updated))
}

fn is_bulk_plan(plan: &ReplacementPlan) -> bool {
    plan.total_match_count > 256
        || plan
            .targets
            .iter()
            .flat_map(|t| &t.replacements)
            .map(|(r, t)| t.len().max(r.len().saturating_mul(4)))
            .sum::<usize>()
            >= crate::app::domain::buffer::BULK_TEXT_BYTES
}
