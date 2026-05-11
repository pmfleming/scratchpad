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
        |request, result_tx| match request {
            BackgroundIoRequest::LoadPaths {
                request_id,
                requests,
                streaming,
            } => {
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
            }
            BackgroundIoRequest::SavePath {
                request_id,
                path,
                snapshot,
                format,
            } => LaneOutcome::result(save_path(request_id, path, snapshot, format)),
            _ => LaneOutcome::Skip,
        },
    );
}

pub(super) fn spawn_session_lane(endpoints: LaneEndpoints) {
    spawn_lane(
        BackgroundIoLane::Session,
        endpoints.request_rx,
        endpoints.result_tx,
        endpoints.lane_depths,
        |request, result_tx| match request {
            BackgroundIoRequest::RestoreSession {
                request_id,
                session_store,
            } => LaneOutcome::HandledWithSendFailure(stream_restore_session(
                request_id,
                session_store,
                result_tx,
            )),
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

fn save_path(
    request_id: u64,
    path: PathBuf,
    snapshot: DocumentSnapshot,
    format: TextFormatMetadata,
) -> BackgroundIoResult {
    let result = FileService::write_snapshot_with_format(&path, &snapshot, &format)
        .map_err(|error| error.to_string());
    let disk_state = result
        .as_ref()
        .ok()
        .and_then(|_| FileService::read_disk_state(&path).ok());
    BackgroundIoResult::PathSaved {
        request_id,
        path,
        disk_state,
        result,
    }
}

fn stream_restore_session(
    request_id: u64,
    session_store: crate::app::services::session_store::SessionStore,
    result_tx: &Sender<BackgroundIoResult>,
) -> bool {
    let send_failed = std::cell::Cell::new(false);
    let result = session_store.load_streaming(
        |active_tab_index, legacy_settings| {
            if result_tx
                .send(BackgroundIoResult::SessionRestoreStarted {
                    request_id,
                    active_tab_index,
                    legacy_settings,
                })
                .is_err()
            {
                send_failed.set(true);
                return false;
            }
            true
        },
        |tab| {
            if result_tx
                .send(BackgroundIoResult::SessionTabRestored { request_id, tab })
                .is_err()
            {
                send_failed.set(true);
                return false;
            }
            true
        },
    );

    if send_failed.get() {
        return true;
    }

    result_tx
        .send(BackgroundIoResult::SessionRestored {
            request_id,
            result: result.map_err(|error| error.to_string()),
        })
        .is_err()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::domain::{BufferState, TextDocument};

    #[test]
    fn load_paths_preserves_input_order_for_parallel_reads() {
        let directory = tempfile::tempdir().unwrap();
        let paths = ["one.txt", "two.txt", "three.txt", "four.txt", "five.txt"]
            .into_iter()
            .map(|name| {
                let path = directory.path().join(name);
                std::fs::write(&path, name).unwrap();
                path
            })
            .collect::<Vec<_>>();
        let requests = paths
            .iter()
            .cloned()
            .map(PathLoadRequest::Standard)
            .collect::<Vec<_>>();

        let results = load_paths(requests);

        assert_eq!(
            results
                .iter()
                .map(|result| &result.path)
                .collect::<Vec<_>>(),
            paths.iter().collect::<Vec<_>>()
        );
        assert!(results.into_iter().all(|result| result.result.is_ok()));
    }

    #[test]
    fn load_one_with_explicit_encoding_marks_loaded_buffer_format() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ansi.txt");
        std::fs::write(&path, [0x63, 0x61, 0x66, 0xe9]).unwrap();

        let result = load_one(PathLoadRequest::WithEncoding {
            path,
            encoding_name: "windows-1252".to_owned(),
        });

        let buffer = result.result.unwrap();
        assert_eq!(buffer.text(), "café");
        assert_eq!(buffer.format.encoding_name, "windows-1252");
    }

    #[test]
    fn refresh_text_metadata_reports_lines_and_control_artifacts() {
        let buffer = BufferState::new("sample.txt".to_owned(), "one\n\u{200e}two".to_owned(), None);
        let snapshot = buffer.document_snapshot();
        let format = buffer.format.clone();

        let (line_count, artifact_summary, refreshed_format) =
            refresh_text_metadata(snapshot, format);

        assert_eq!(line_count, 2);
        assert!(artifact_summary.has_control_chars());
        assert_eq!(refreshed_format.preferred_line_ending.as_str(), "\n");
    }

    #[test]
    fn refresh_encoding_compliance_detects_unencodable_snapshot_text() {
        let document = TextDocument::new("plain 😀".to_owned());
        let snapshot = document.snapshot();
        let format = TextFormatMetadata::detected(
            "plain",
            "windows-1252".to_owned(),
            false,
            crate::app::domain::EncodingSource::Heuristic,
            false,
        );

        assert!(
            format.has_non_compliant_characters_spans(
                snapshot
                    .piece_tree()
                    .spans_for_range(0..snapshot.document_length().chars)
                    .map(|span| span.text)
            )
        );
    }
}
