use super::dispatcher::LaneDepths;
use super::types::{BackgroundIoRequest, BackgroundIoResult, LoadedPathResult, PathLoadRequest};
use crate::app::capacity_metrics::{self, BackgroundIoLane};
use crate::app::domain::{DocumentSnapshot, TextArtifactSummary, TextFormatMetadata};
use crate::app::services::file_service::{FileContent, FileService};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

pub(super) struct LaneEndpoints {
    pub(super) request_rx: Receiver<BackgroundIoRequest>,
    pub(super) result_tx: Sender<BackgroundIoResult>,
    pub(super) lane_depths: Arc<LaneDepths>,
}

/// Outcome of a per-request handler.
enum LaneOutcome {
    /// Send `result` over `result_tx`; if that fails, terminate the lane.
    Result(Box<BackgroundIoResult>),
    /// Handler emitted its own messages; continue if `false`, terminate if `true`.
    HandledWithSendFailure(bool),
    /// Skip this request (wrong lane) — keep running.
    Skip,
}

impl LaneOutcome {
    fn result(result: BackgroundIoResult) -> Self {
        Self::Result(Box::new(result))
    }
}

fn spawn_lane(
    lane: BackgroundIoLane,
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
    lane_depths: Arc<LaneDepths>,
    handle: impl Fn(BackgroundIoRequest, &Sender<BackgroundIoResult>) -> LaneOutcome + Send + 'static,
) {
    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            lane_depths.decrement(lane);
            let started_at = Instant::now();
            let send_failed = match handle(request, &result_tx) {
                LaneOutcome::Result(result) => result_tx.send(*result).is_err(),
                LaneOutcome::HandledWithSendFailure(failed) => failed,
                LaneOutcome::Skip => continue,
            };
            if send_failed {
                break;
            }
            capacity_metrics::record_background_io_lane(lane, started_at.elapsed());
        }
    });
}

pub(super) fn spawn_path_lane(endpoints: LaneEndpoints) {
    spawn_lane(
        BackgroundIoLane::Path,
        endpoints.request_rx,
        endpoints.result_tx,
        endpoints.lane_depths,
        |request, result_tx| {
            let BackgroundIoRequest::LoadPaths {
                request_id,
                requests,
                streaming,
            } = request
            else {
                return LaneOutcome::Skip;
            };
            if streaming && requests.len() > 1 {
                LaneOutcome::HandledWithSendFailure(stream_load_paths(
                    request_id, requests, result_tx,
                ))
            } else {
                LaneOutcome::result(BackgroundIoResult::PathsLoaded {
                    request_id,
                    results: load_paths(requests),
                    is_partial: false,
                })
            }
        },
    );
}

pub(super) fn spawn_session_lane(endpoints: LaneEndpoints) {
    spawn_lane(
        BackgroundIoLane::Session,
        endpoints.request_rx,
        endpoints.result_tx,
        endpoints.lane_depths,
        |request, _| match request {
            BackgroundIoRequest::RestoreSession {
                request_id,
                session_store,
            } => LaneOutcome::result(BackgroundIoResult::SessionRestored {
                request_id,
                result: session_store.load().map_err(|error| error.to_string()),
            }),
            BackgroundIoRequest::PersistSession {
                request_id,
                session_store,
                request,
            } => LaneOutcome::result(BackgroundIoResult::SessionPersisted {
                request_id,
                result: session_store
                    .persist_request(request)
                    .map_err(|error| error.to_string()),
            }),
            _ => LaneOutcome::Skip,
        },
    );
}

pub(super) fn spawn_analysis_lane(endpoints: LaneEndpoints) {
    spawn_lane(
        BackgroundIoLane::Analysis,
        endpoints.request_rx,
        endpoints.result_tx,
        endpoints.lane_depths,
        |request, _| match request {
            BackgroundIoRequest::RefreshTextMetadata {
                request_id,
                buffer_id,
                revision,
                snapshot,
                format,
            } => LaneOutcome::result(BackgroundIoResult::TextMetadataRefreshed {
                request_id,
                buffer_id,
                revision,
                result: Ok(refresh_text_metadata(snapshot, format)),
            }),
            BackgroundIoRequest::RefreshEncodingCompliance {
                request_id,
                buffer_id,
                revision,
                snapshot,
                format,
            } => LaneOutcome::result(BackgroundIoResult::EncodingComplianceRefreshed {
                request_id,
                buffer_id,
                revision,
                result: Ok(format.has_non_compliant_characters_spans(
                    snapshot
                        .piece_tree()
                        .spans_for_range(0..snapshot.document_length().chars)
                        .map(|span| span.text),
                )),
            }),
            _ => LaneOutcome::Skip,
        },
    );
}

// Open multiple files concurrently to exploit SSD throughput while keeping
// memory pressure bounded. Modern NVMe SSDs benefit from a handful of
// concurrent reads; we cap the fanout to keep the bursts predictable.
const MAX_CONCURRENT_READS: usize = 4;

/// Fan `requests` out across up to `MAX_CONCURRENT_READS` scoped workers,
/// invoking `consume` on each `(index, result)` tuple as it arrives. The
/// scope blocks until all workers complete.
fn for_each_loaded(
    requests: Vec<PathLoadRequest>,
    mut consume: impl FnMut(usize, LoadedPathResult),
) {
    let total = requests.len();
    if total == 0 {
        return;
    }
    let worker_count = thread::available_parallelism()
        .map(|p| p.get().min(MAX_CONCURRENT_READS))
        .unwrap_or(2)
        .min(total);
    let chunk_size = total.div_ceil(worker_count);
    let mut iter = requests.into_iter().enumerate();
    let (load_tx, load_rx) = mpsc::channel::<(usize, LoadedPathResult)>();

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let chunk = iter.by_ref().take(chunk_size).collect::<Vec<_>>();
            if chunk.is_empty() {
                break;
            }
            let load_tx = load_tx.clone();
            scope.spawn(move || {
                for (index, request) in chunk {
                    if load_tx.send((index, load_one(request))).is_err() {
                        return;
                    }
                }
            });
        }
        drop(load_tx);

        for (index, result) in load_rx {
            consume(index, result);
        }
    });
}

/// Stream individual `PathsLoaded` messages as each path finishes loading,
/// preserving input order. The first N-1 messages carry `is_partial: true`;
/// the final one carries `is_partial: false`. Returns `true` if a send
/// failure terminates the lane worker.
fn stream_load_paths(
    request_id: u64,
    requests: Vec<PathLoadRequest>,
    result_tx: &Sender<BackgroundIoResult>,
) -> bool {
    let total = requests.len();
    debug_assert!(total > 1);
    // Drain in input-index order so the active tab (index 0) installs first
    // and downstream code sees a deterministic order across runs.
    let mut pending: std::collections::HashMap<usize, LoadedPathResult> =
        std::collections::HashMap::new();
    let mut next_index = 0usize;
    let mut emitted = 0usize;
    let mut send_failed = false;

    for_each_loaded(requests, |index, result| {
        if send_failed {
            return;
        }
        pending.insert(index, result);
        while let Some(result) = pending.remove(&next_index) {
            next_index += 1;
            emitted += 1;
            let is_partial = emitted < total;
            if result_tx
                .send(BackgroundIoResult::PathsLoaded {
                    request_id,
                    results: vec![result],
                    is_partial,
                })
                .is_err()
            {
                send_failed = true;
                return;
            }
        }
    });

    send_failed
}

fn load_paths(requests: Vec<PathLoadRequest>) -> Vec<LoadedPathResult> {
    let total = requests.len();
    if total <= 1 {
        return requests.into_iter().map(load_one).collect();
    }
    let mut indexed_results: Vec<(usize, LoadedPathResult)> = Vec::with_capacity(total);
    for_each_loaded(requests, |index, result| {
        indexed_results.push((index, result));
    });
    indexed_results.sort_by_key(|(index, _)| *index);
    indexed_results
        .into_iter()
        .map(|(_, result)| result)
        .collect()
}

fn load_one(request: PathLoadRequest) -> LoadedPathResult {
    match request {
        PathLoadRequest::Standard(path) => load_path_result(path, FileService::read_file),
        PathLoadRequest::WithEncoding {
            path,
            encoding_name,
        } => load_path_result(path, |path| {
            FileService::read_file_with_encoding(path, &encoding_name)
        }),
    }
}

fn load_path_result(
    path: PathBuf,
    read_file: impl FnOnce(&Path) -> io::Result<FileContent>,
) -> LoadedPathResult {
    let disk_state = FileService::read_disk_state(&path).ok();
    let result = read_file(&path)
        .map(|file_content| {
            FileService::build_buffer_from_file_content(&path, file_content, disk_state.clone())
        })
        .map_err(|error| error.to_string());
    LoadedPathResult {
        path,
        disk_state,
        result,
    }
}

fn refresh_text_metadata(
    snapshot: DocumentSnapshot,
    mut format: TextFormatMetadata,
) -> (usize, TextArtifactSummary, TextFormatMetadata) {
    let metadata = crate::app::domain::buffer::buffer_text_metadata_from_piece_tree(
        snapshot.piece_tree(),
        &mut format,
    );
    (metadata.line_count, metadata.artifact_summary, format)
}
