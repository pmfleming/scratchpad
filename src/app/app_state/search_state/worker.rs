use super::{SearchMatch, SearchResultGroup, SearchStatus};
use crate::app::capacity_metrics;
use crate::app::domain::{BufferId, DocumentSnapshot, ViewId};
use crate::app::services::search::SearchOptions;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, atomic::AtomicU64};
use std::thread;
use std::time::Instant;

mod processing;

pub(super) use processing::{process_search_request, process_search_request_with_partials};

pub(super) struct SearchRequest {
    pub(super) generation: u64,
    pub(super) query: String,
    pub(super) options: SearchOptions,
    pub(super) targets: Vec<SearchTargetSnapshot>,
}

pub(super) struct SearchResult {
    pub(super) generation: u64,
    pub(super) matches: Vec<SearchMatch>,
    pub(super) result_groups: Vec<SearchResultGroup>,
    pub(super) displayed_match_count: usize,
    pub(super) status: SearchStatus,
}

pub(super) struct SearchTargetSnapshot {
    pub(super) file_identity: SearchFileIdentity,
    pub(super) tab_index: usize,
    pub(super) view_id: ViewId,
    pub(super) buffer_id: BufferId,
    pub(super) tab_label: String,
    pub(super) buffer_label: String,
    pub(super) document_snapshot: DocumentSnapshot,
    pub(super) search_range: Option<Range<usize>>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) enum SearchFileIdentity {
    Path(PathBuf),
    Untitled(BufferId),
}

pub(super) fn spawn_search_worker(
    latest_generation: Arc<AtomicU64>,
) -> (Sender<SearchRequest>, Receiver<SearchResult>) {
    let (request_tx, request_rx) = mpsc::channel::<SearchRequest>();
    let (result_tx, result_rx) = mpsc::channel::<SearchResult>();
    thread::spawn(move || {
        for request in &request_rx {
            let (request, coalesced_queue_depth) = request_rx
                .try_iter()
                .fold((request, 1), |(_, depth), latest| (latest, depth + 1));
            capacity_metrics::record_search_request(request.targets.len(), coalesced_queue_depth);
            let started_at = Instant::now();
            let mut partial_failed = false;
            let mut partial_emit = |partial: SearchResult| {
                if !partial_failed {
                    partial_failed = result_tx.send(partial).is_err();
                }
            };
            if let Some(result) = process_search_request_with_partials(
                request,
                &latest_generation,
                Some(&mut partial_emit),
            ) && result_tx.send(result).is_err()
            {
                break;
            }
            capacity_metrics::record_search_worker_active(started_at.elapsed());
        }
    });
    (request_tx, result_rx)
}
