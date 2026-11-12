//! A complete replacement review is built independently of the bounded result
//! list. Only one target's search ranges are materialized at a time.
use super::super::{ReplaceAllConfirmation, ReplacementPlan, ReplacementTargetPlan, ScratchpadApp};
use crate::app::services::search::SearchProgram;
use std::sync::mpsc::{self, Receiver};

pub(crate) struct PendingPlan {
    rx: Receiver<Result<ReplacementPlan, String>>,
    generation: u64,
    replacement: String,
    bases: Vec<(u64, crate::app::domain::buffer::HistoryToken)>,
}
pub(crate) fn start(app: &mut ScratchpadApp) -> bool {
    app.state.search_state.replace.last_operation = None;
    let state = &app.state.search_state;
    if state.replace.planning.is_some() || state.replace.pending.is_some() {
        return false;
    }
    if let Some(review) = &state.replace.pending_replace_all_confirmation {
        let plan = review.plan.clone();
        app.state
            .search_state
            .replace
            .pending_replace_all_confirmation = None;
        return super::transaction::start(app, plan);
    }
    let scope = state.query.scope;
    let selection = super::super::selection::resolve(app);
    let targets = super::super::runtime::collect_search_targets(app, scope);
    let mut buffers = Vec::new();
    let mut bases = Vec::new();
    for target in targets {
        let Some(buffer) = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .find_map(|t| t.buffer_by_id(target.buffer_id))
        else {
            return false;
        };
        if buffer.is_loading_preview || buffer.is_preparing_text() {
            return false;
        }
        bases.push((buffer.id, buffer.document().history_token()));
        buffers.push((target, buffer.clone()));
    }
    let generation = state.runtime.requested_generation;
    let query = state.query.query.clone();
    let options = state.search_options();
    let replacement = state.query.replacement.clone();
    let replacement_for_worker = replacement.clone();
    let preserve = state.query.preserve_case;
    let repaint = state.runtime.repaint.clone();
    let (tx, rx) = mpsc::channel();
    let scan = ReplacementScan {
        buffers,
        scope,
        selection,
        query,
        options,
        replacement: replacement_for_worker,
        preserve,
    };
    let worker = move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scan.build()))
            .unwrap_or_else(|_| Err("Text became unreadable while reviewing replacement".into()));
        let _ = tx.send(result);
        repaint.notify();
    };
    match std::thread::Builder::new()
        .name("search-replace-review".into())
        .spawn(worker)
    {
        Ok(_) => {
            app.state.search_state.replace.planning = Some(PendingPlan {
                rx,
                generation,
                replacement,
                bases,
            });
            app.state.search_state.replace.outcome =
                Some("Counting all matches for review…".into());
            true
        }
        Err(error) => {
            app.state.search_state.replace.outcome =
                Some(format!("Could not review replacement: {error}"));
            false
        }
    }
}
pub(crate) fn poll(app: &mut ScratchpadApp) {
    let Some(pending) = app.state.search_state.replace.planning.as_ref() else {
        return;
    };
    let result = match pending.rx.try_recv() {
        Err(mpsc::TryRecvError::Empty) => return,
        Err(_) => Err("Replacement review stopped".into()),
        Ok(r) => r,
    };
    let Some(pending) = app.state.search_state.replace.planning.take() else {
        return;
    };
    let current = pending.generation == app.state.search_state.runtime.requested_generation
        && pending.replacement == app.state.search_state.query.replacement
        && pending.bases.iter().all(|(id, token)| {
            app.tab_manager
                .tabs
                .as_slice()
                .iter()
                .find_map(|t| t.buffer_by_id(*id))
                .is_some_and(|b| b.document().history_token() == *token)
        });
    match result {
        Ok(plan) if current => {
            app.state.search_state.replace.outcome = Some(format!(
                "Review all {} matches beyond the result limit.",
                plan.total_match_count
            ));
            app.state
                .search_state
                .replace
                .pending_replace_all_confirmation = Some(ReplaceAllConfirmation::from_plan(
                &plan,
                &pending.replacement,
                pending.generation,
            ));
        }
        Ok(_) => {
            app.state.search_state.replace.outcome =
                Some("Files or search changed. Build a new review before replacing.".into());
        }
        Err(message) => app.state.search_state.replace.outcome = Some(message),
    }
}

struct ReplacementScan {
    buffers: Vec<(
        super::super::worker::SearchTargetSnapshot,
        crate::app::domain::BufferState,
    )>,
    scope: super::super::SearchScope,
    selection: Option<(usize, u64, u64, std::ops::Range<usize>)>,
    query: String,
    options: crate::app::services::search::SearchOptions,
    replacement: String,
    preserve: bool,
}
impl ReplacementScan {
    fn build(self) -> Result<ReplacementPlan, String> {
        let Self {
            buffers,
            scope,
            selection,
            query,
            options,
            replacement: replacement_for_worker,
            preserve,
        } = self;
        let program =
            SearchProgram::compile(&query, options).map_err(|e| e.message().to_owned())?;
        let mut plan = ReplacementPlan {
            scope,
            targets: Vec::new(),
            total_match_count: 0,
        };
        let generation = std::sync::atomic::AtomicU64::new(1);
        for (target, buffer) in buffers {
            let ranges = super::super::fragments::search_target_ranges(
                &target.document_snapshot,
                target.search_range.clone(),
                &program,
                1,
                &generation,
                1,
            )
            .ok_or_else(|| "Could not read a replacement target".to_owned())?;
            if ranges.is_empty() {
                continue;
            }
            let mut replacements = Vec::with_capacity(ranges.len());
            for range in ranges.into_iter().rev() {
                let context_range = selection
                    .as_ref()
                    .filter(|(_, id, _, _)| *id == buffer.id)
                    .map(|(_, _, _, r)| r.clone());
                let expanded = super::expand_for_buffer_in_range(
                    &program,
                    &buffer,
                    &range,
                    &replacement_for_worker,
                    preserve,
                    context_range,
                )
                .map_err(|e| e.message().to_owned())?;
                replacements.push((range, expanded));
            }
            plan.total_match_count += replacements.len();
            plan.targets.push(ReplacementTargetPlan {
                tab_index: target.tab_index,
                view_id: target.view_id,
                buffer_id: buffer.id,
                buffer_label: target.buffer_label,
                target_revision: target.document_snapshot.revision(),
                target_identity: target.document_snapshot.revision_key(),
                replacements,
            });
        }
        Ok(plan)
    }
}
