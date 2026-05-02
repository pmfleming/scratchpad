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
}

impl BackgroundIoSendError {
    fn from_try_send_error(error: TrySendError<BackgroundIoRequest>) -> Self {
        let request = match error {
            TrySendError::Full(request) | TrySendError::Disconnected(request) => request,
        };
        Self {
            request: Box::new(request),
        }
    }

    pub(crate) fn into_request(self) -> BackgroundIoRequest {
        *self.request
    }
}

impl BackgroundIoDispatcher {
    pub(crate) fn send(&self, request: BackgroundIoRequest) -> Result<(), BackgroundIoSendError> {
        // Increment BEFORE try_send so the receiving worker can never observe
        // a decrement before the corresponding increment (and underflow the
        // counter into u64::MAX). Roll back on failure.
        let lane = match request {
            BackgroundIoRequest::LoadPaths { .. } => BackgroundIoLane::Path,
            BackgroundIoRequest::RestoreSession { .. }
            | BackgroundIoRequest::PersistSession { .. } => BackgroundIoLane::Session,
            BackgroundIoRequest::RefreshTextMetadata { .. }
            | BackgroundIoRequest::RefreshEncodingCompliance { .. } => BackgroundIoLane::Analysis,
        };
        self.lane_depths.increment(lane);
        let tx = match lane {
            BackgroundIoLane::Path => &self.path_tx,
            BackgroundIoLane::Session => &self.session_tx,
            BackgroundIoLane::Analysis => &self.analysis_tx,
        };
        match tx.try_send(request) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.lane_depths.decrement(lane);
                if matches!(error, TrySendError::Full(_)) {
                    capacity_metrics::record_background_io_saturation(lane);
                }
                Err(BackgroundIoSendError::from_try_send_error(error))
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

fn spawn_path_lane(
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
    lane_depths: Arc<LaneDepths>,
) {
    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            lane_depths.decrement(BackgroundIoLane::Path);
            let started_at = Instant::now();
            let BackgroundIoRequest::LoadPaths {
                request_id,
                requests,
                streaming,
            } = request
            else {
                continue;
            };
            let send_failed = if streaming && requests.len() > 1 {
                stream_load_paths(request_id, requests, &result_tx)
            } else {
                result_tx
                    .send(BackgroundIoResult::PathsLoaded {
                        request_id,
                        results: load_paths(requests),
                        is_partial: false,
                    })
                    .is_err()
            };
            if send_failed {
                break;
            }
            capacity_metrics::record_background_io_lane(
                BackgroundIoLane::Path,
                started_at.elapsed(),
            );
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
    // Run loads on scoped workers; route results through an mpsc channel so
    // the foreground worker can release them in input order, and emit
    // streaming `PathsLoaded` messages as each is ready.
    const MAX_CONCURRENT_READS: usize = 4;
    let worker_count = thread::available_parallelism()
        .map(|p| p.get().min(MAX_CONCURRENT_READS))
        .unwrap_or(2)
        .min(total);
    let indexed = requests.into_iter().enumerate().collect::<Vec<_>>();
    let chunk_size = indexed.len().div_ceil(worker_count);
    let mut iter = indexed.into_iter();
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
                    let result = load_one(request);
                    if load_tx.send((index, result)).is_err() {
                        return;
                    }
                }
            });
        }
        drop(load_tx);
    });

    // Drain in input-index order so the active tab (index 0) installs first
    // and downstream code sees a deterministic order across runs.
    let mut pending: std::collections::HashMap<usize, LoadedPathResult> =
        std::collections::HashMap::new();
    let mut next_index = 0usize;
    let mut emitted = 0usize;
    for (index, result) in load_rx {
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
                return true;
            }
        }
    }
    false
}

fn spawn_session_lane(
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
    lane_depths: Arc<LaneDepths>,
) {
    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            lane_depths.decrement(BackgroundIoLane::Session);
            let started_at = Instant::now();
            let result = match request {
                BackgroundIoRequest::RestoreSession {
                    request_id,
                    session_store,
                } => BackgroundIoResult::SessionRestored {
                    request_id,
                    result: session_store.load().map_err(|error| error.to_string()),
                },
                BackgroundIoRequest::PersistSession {
                    request_id,
                    session_store,
                    request,
                } => BackgroundIoResult::SessionPersisted {
                    request_id,
                    result: session_store
                        .persist_request(request)
                        .map_err(|error| error.to_string()),
                },
                BackgroundIoRequest::LoadPaths { .. }
                | BackgroundIoRequest::RefreshTextMetadata { .. }
                | BackgroundIoRequest::RefreshEncodingCompliance { .. } => continue,
            };

            if result_tx.send(result).is_err() {
                break;
            }
            capacity_metrics::record_background_io_lane(
                BackgroundIoLane::Session,
                started_at.elapsed(),
            );
        }
    });
}

fn spawn_analysis_lane(
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
    lane_depths: Arc<LaneDepths>,
) {
    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            lane_depths.decrement(BackgroundIoLane::Analysis);
            let started_at = Instant::now();
            let result = match request {
                BackgroundIoRequest::RefreshTextMetadata {
                    request_id,
                    buffer_id,
                    revision,
                    snapshot,
                    format,
                } => BackgroundIoResult::TextMetadataRefreshed {
                    request_id,
                    buffer_id,
                    revision,
                    result: Ok(refresh_text_metadata(snapshot, format)),
                },
                BackgroundIoRequest::RefreshEncodingCompliance {
                    request_id,
                    buffer_id,
                    revision,
                    snapshot,
                    format,
                } => BackgroundIoResult::EncodingComplianceRefreshed {
                    request_id,
                    buffer_id,
                    revision,
                    result: Ok(format.has_non_compliant_characters_spans(
                        snapshot
                            .piece_tree()
                            .spans_for_range(0..snapshot.document_length().chars)
                            .map(|span| span.text),
                    )),
                },
                BackgroundIoRequest::LoadPaths { .. }
                | BackgroundIoRequest::RestoreSession { .. }
                | BackgroundIoRequest::PersistSession { .. } => continue,
            };

            if result_tx.send(result).is_err() {
                break;
            }
            capacity_metrics::record_background_io_lane(
                BackgroundIoLane::Analysis,
                started_at.elapsed(),
            );
        }
    });
}

fn load_paths(requests: Vec<PathLoadRequest>) -> Vec<LoadedPathResult> {
    // Open multiple files concurrently to exploit SSD throughput while keeping
    // memory pressure bounded. Modern NVMe SSDs benefit from a handful of
    // concurrent reads; we cap the fanout to keep the bursts predictable.
    const MAX_CONCURRENT_READS: usize = 4;
    let total = requests.len();
    if total <= 1 {
        return requests.into_iter().map(load_one).collect();
    }
    let worker_count = thread::available_parallelism()
        .map(|p| p.get().min(MAX_CONCURRENT_READS))
        .unwrap_or(2)
        .min(total);
    let indexed = requests.into_iter().enumerate().collect::<Vec<_>>();
    let chunk_size = indexed.len().div_ceil(worker_count);
    let mut iter = indexed.into_iter();
    let mut indexed_results: Vec<(usize, LoadedPathResult)> = Vec::with_capacity(total);
    thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..worker_count {
            let chunk = iter.by_ref().take(chunk_size).collect::<Vec<_>>();
            if chunk.is_empty() {
                break;
            }
            handles.push(scope.spawn(move || {
                chunk
                    .into_iter()
                    .map(|(index, request)| (index, load_one(request)))
                    .collect::<Vec<_>>()
            }));
        }
        for handle in handles {
            let mut chunk_results = handle.join().expect("path load worker panicked");
            indexed_results.append(&mut chunk_results);
        }
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
