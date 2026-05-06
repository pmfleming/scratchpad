use crate::app::capacity_metrics::{self, BackgroundIoLane};
use crate::app::domain::{
    BufferState, DiskFileState, DocumentSnapshot, TextArtifactSummary, TextFormatMetadata,
};
use crate::app::services::file_service::{FileContent, FileService};
use crate::app::services::session_store::{RestoredSession, SessionPersistRequest, SessionStore};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::thread;
use std::time::Instant;

pub(crate) enum PathLoadRequest {
    Standard(PathBuf),
    WithEncoding {
        path: PathBuf,
        encoding_name: String,
    },
}

impl PathLoadRequest {
    pub(crate) fn path(&self) -> &PathBuf {
        match self {
            Self::Standard(path) => path,
            Self::WithEncoding { path, .. } => path,
        }
    }
}

pub(crate) enum BackgroundIoRequest {
    LoadPaths {
        request_id: u64,
        requests: Vec<PathLoadRequest>,
        /// When true, each path's `LoadedPathResult` is streamed back as a
        /// separate `PathsLoaded { is_partial: true }` message; the final
        /// path is delivered with `is_partial: false`. When false, all
        /// results are batched into one terminal `PathsLoaded` message.
        streaming: bool,
    },
    RestoreSession {
        request_id: u64,
        session_store: SessionStore,
    },
    PersistSession {
        request_id: u64,
        session_store: SessionStore,
        request: SessionPersistRequest,
    },
    RefreshTextMetadata {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        snapshot: DocumentSnapshot,
        format: TextFormatMetadata,
    },
    RefreshEncodingCompliance {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        snapshot: DocumentSnapshot,
        format: TextFormatMetadata,
    },
}

pub(crate) enum BackgroundIoResult {
    PathsLoaded {
        request_id: u64,
        results: Vec<LoadedPathResult>,
        /// When true, more `PathsLoaded` messages will follow for this
        /// `request_id`; the action stays in `pending_background_actions`.
        /// When false (terminal), the action is removed and finalized.
        is_partial: bool,
    },
    SessionRestored {
        request_id: u64,
        result: Result<Option<RestoredSession>, String>,
    },
    SessionPersisted {
        request_id: u64,
        result: Result<(), String>,
    },
    TextMetadataRefreshed {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        result: Result<(usize, TextArtifactSummary, TextFormatMetadata), String>,
    },
    EncodingComplianceRefreshed {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        result: Result<bool, String>,
    },
}

pub(crate) struct LoadedPathResult {
    pub(crate) path: PathBuf,
    pub(crate) disk_state: Option<DiskFileState>,
    pub(crate) result: Result<BufferState, String>,
}

pub(crate) struct BackgroundIoDispatcher {
    path_tx: SyncSender<BackgroundIoRequest>,
    session_tx: SyncSender<BackgroundIoRequest>,
    analysis_tx: SyncSender<BackgroundIoRequest>,
    lane_depths: Arc<LaneDepths>,
}

#[derive(Default)]
struct LaneDepths {
    path: AtomicU64,
    session: AtomicU64,
    analysis: AtomicU64,
}

impl LaneDepths {
    fn counter(&self, lane: BackgroundIoLane) -> &AtomicU64 {
        match lane {
            BackgroundIoLane::Path => &self.path,
            BackgroundIoLane::Session => &self.session,
            BackgroundIoLane::Analysis => &self.analysis,
        }
    }

    fn increment(&self, lane: BackgroundIoLane) {
        let depth = self.counter(lane).fetch_add(1, Ordering::Relaxed) + 1;
        capacity_metrics::record_background_io_queue_depth(lane, depth as usize);
    }

    fn decrement(&self, lane: BackgroundIoLane) {
        self.counter(lane).fetch_sub(1, Ordering::Relaxed);
    }
}

pub(crate) struct BackgroundIoSendError {
    request: Box<BackgroundIoRequest>,
    lane: BackgroundIoLane,
    reason: &'static str,
    request_kind: &'static str,
}

impl BackgroundIoSendError {
    fn from_try_send_error(
        error: TrySendError<BackgroundIoRequest>,
        lane: BackgroundIoLane,
    ) -> Self {
        let (reason, request) = match error {
            TrySendError::Full(request) => ("full", request),
            TrySendError::Disconnected(request) => ("disconnected", request),
        };
        let request_kind = request.kind();
        Self {
            request: Box::new(request),
            lane,
            reason,
            request_kind,
        }
    }

    pub(crate) fn lane_name(&self) -> &'static str {
        lane_name(self.lane)
    }

    pub(crate) fn reason(&self) -> &'static str {
        self.reason
    }

    pub(crate) fn request_kind(&self) -> &'static str {
        self.request_kind
    }

    pub(crate) fn into_request(self) -> BackgroundIoRequest {
        *self.request
    }
}

impl BackgroundIoRequest {
    fn kind(&self) -> &'static str {
        match self {
            Self::LoadPaths { .. } => "load_paths",
            Self::RestoreSession { .. } => "restore_session",
            Self::PersistSession { .. } => "persist_session",
            Self::RefreshTextMetadata { .. } => "refresh_text_metadata",
            Self::RefreshEncodingCompliance { .. } => "refresh_encoding_compliance",
        }
    }

    fn lane(&self) -> BackgroundIoLane {
        match self {
            Self::LoadPaths { .. } => BackgroundIoLane::Path,
            Self::RestoreSession { .. } | Self::PersistSession { .. } => BackgroundIoLane::Session,
            Self::RefreshTextMetadata { .. } | Self::RefreshEncodingCompliance { .. } => {
                BackgroundIoLane::Analysis
            }
        }
    }
}

fn lane_name(lane: BackgroundIoLane) -> &'static str {
    match lane {
        BackgroundIoLane::Path => "path",
        BackgroundIoLane::Session => "session",
        BackgroundIoLane::Analysis => "analysis",
    }
}

impl BackgroundIoDispatcher {
    pub(crate) fn send(&self, request: BackgroundIoRequest) -> Result<(), BackgroundIoSendError> {
        // Increment BEFORE try_send so the receiving worker can never observe
        // a decrement before the corresponding increment (and underflow the
        // counter into u64::MAX). Roll back on failure.
        let lane = request.lane();
        let tx = match lane {
            BackgroundIoLane::Path => &self.path_tx,
            BackgroundIoLane::Session => &self.session_tx,
            BackgroundIoLane::Analysis => &self.analysis_tx,
        };
        self.lane_depths.increment(lane);
        match tx.try_send(request) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.lane_depths.decrement(lane);
                if matches!(error, TrySendError::Full(_)) {
                    capacity_metrics::record_background_io_saturation(lane);
                }
                Err(BackgroundIoSendError::from_try_send_error(error, lane))
            }
        }
    }
}

const PATH_LANE_QUEUE_BOUND: usize = 8;
const SESSION_LANE_QUEUE_BOUND: usize = 2;
const ANALYSIS_LANE_QUEUE_BOUND: usize = 16;

pub(crate) fn spawn_background_io_worker() -> (BackgroundIoDispatcher, Receiver<BackgroundIoResult>)
{
    let (result_tx, result_rx) = mpsc::channel::<BackgroundIoResult>();
    let (path_tx, path_rx) = mpsc::sync_channel::<BackgroundIoRequest>(PATH_LANE_QUEUE_BOUND);
    let (session_tx, session_rx) =
        mpsc::sync_channel::<BackgroundIoRequest>(SESSION_LANE_QUEUE_BOUND);
    let (analysis_tx, analysis_rx) =
        mpsc::sync_channel::<BackgroundIoRequest>(ANALYSIS_LANE_QUEUE_BOUND);

    let lane_depths = Arc::new(LaneDepths::default());
    spawn_path_lane(path_rx, result_tx.clone(), Arc::clone(&lane_depths));
    spawn_session_lane(session_rx, result_tx.clone(), Arc::clone(&lane_depths));
    spawn_analysis_lane(analysis_rx, result_tx, Arc::clone(&lane_depths));

    (
        BackgroundIoDispatcher {
            path_tx,
            session_tx,
            analysis_tx,
            lane_depths,
        },
        result_rx,
    )
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

fn spawn_path_lane(
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
    lane_depths: Arc<LaneDepths>,
) {
    spawn_lane(
        BackgroundIoLane::Path,
        request_rx,
        result_tx,
        lane_depths,
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

fn spawn_session_lane(
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
    lane_depths: Arc<LaneDepths>,
) {
    spawn_lane(
        BackgroundIoLane::Session,
        request_rx,
        result_tx,
        lane_depths,
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

fn spawn_analysis_lane(
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
    lane_depths: Arc<LaneDepths>,
) {
    spawn_lane(
        BackgroundIoLane::Analysis,
        request_rx,
        result_tx,
        lane_depths,
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
