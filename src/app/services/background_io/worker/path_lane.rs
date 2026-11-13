use super::{LaneEndpoints, LaneOutcome, spawn_lane};
use crate::app::capacity_metrics::BackgroundIoLane;
use crate::app::domain::buffer::PrepareTextRequest;
use crate::app::domain::{DocumentSnapshot, PieceSource, TextFormatMetadata, WorkspaceTab};
use crate::app::services::background_io::types::{
    BackgroundIoRequest, BackgroundIoResult, ColdFileShellResult, LoadedPathResult,
    PathLoadRequest, RestoredTabResult,
};
use crate::app::services::chunk_store::{ChunkStoreHandle, ImportContext};
use crate::app::services::file_service::{FileContent, FileService};
use crate::app::services::repaint::WakeSender as Sender;
use crate::app::services::session_store::{RetainedTarget, SessionStore};
use crate::app::{CanonicalPathKey, domain::BufferState};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

pub(in crate::app::services::background_io) fn spawn_path_lane(endpoints: LaneEndpoints) {
    spawn_lane(
        BackgroundIoLane::Path,
        endpoints,
        |request, result_tx| match request {
            BackgroundIoRequest::PrepareText {
                request_id,
                buffer_id,
                session_store,
                request,
            } => LaneOutcome::result(BackgroundIoResult::TextPrepared {
                request_id,
                buffer_id,
                result: prepare_text(*request, &session_store),
            }),
            BackgroundIoRequest::ReopenRetained {
                request_id,
                session_store,
                target,
                claimed,
            } => LaneOutcome::result(BackgroundIoResult::RetainedReopened {
                request_id,
                result: reopen_retained(&session_store, target, &claimed),
            }),
            BackgroundIoRequest::LoadPaths {
                request_id,
                requests,
                streaming,
            } if streaming && requests.len() > 1 => LaneOutcome::HandledWithSendFailure(
                stream_load_paths(request_id, requests, result_tx),
            ),
            BackgroundIoRequest::LoadPaths {
                request_id,
                requests,
                ..
            } => LaneOutcome::result(BackgroundIoResult::PathsLoaded {
                request_id,
                results: load_paths(requests),
                is_partial: false,
            }),
            BackgroundIoRequest::BuildColdFileShells {
                request_id,
                paths,
                session_store,
            } => LaneOutcome::HandledWithSendFailure(stream_cold_file_shells(
                request_id,
                paths,
                &session_store,
                result_tx,
            )),
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

/// The session's chunk store, opening it when possible and otherwise reusing
/// one that is already open (for example while writes are blocked).
fn available_chunk_store(session_store: &SessionStore) -> Option<ChunkStoreHandle> {
    session_store
        .chunk_store_handle()
        .ok()
        .or_else(|| session_store.opened_chunk_store())
}

fn prepare_text(
    request: PrepareTextRequest,
    session_store: &SessionStore,
) -> Result<Box<crate::app::domain::buffer::PreparedText>, String> {
    request
        .prepare_with_store(available_chunk_store(session_store))
        .map(Box::new)
        .map_err(|cause| cause.to_string())
}

fn reopen_retained(
    session_store: &SessionStore,
    target: RetainedTarget,
    claimed: &[CanonicalPathKey],
) -> RestoredTabResult {
    match target {
        RetainedTarget::Cold(cold) => session_store.restore_cold_session_tab(cold),
        target => session_store
            .reopen_target(target, claimed)
            .map(|tab| (tab, None)),
    }
    .map(|(tab, status)| (Box::new(tab), status))
    .map_err(|e| e.to_string())
}

fn stream_cold_file_shells(
    request_id: u64,
    mut paths: Vec<PathBuf>,
    store: &SessionStore,
    result_tx: &Sender<BackgroundIoResult>,
) -> bool {
    let selected_path = paths.pop();
    if let Some(path) = &selected_path {
        let selected = load_one(PathLoadRequest::Retained {
            path: path.clone(),
            session_store: store.clone(),
            claimed: Vec::new(),
        });
        if result_tx
            .send(BackgroundIoResult::PathsLoaded {
                request_id,
                results: vec![selected],
                is_partial: true,
            })
            .is_err()
        {
            return true;
        }
    }
    let shells = paths
        .into_iter()
        .map(|path| match store.cold_tab_for_path(&path) {
            Ok(Some(result)) => ColdFileShellResult {
                path,
                result: Ok(result),
            },
            Ok(None) => build_cold_file_shell(path, store),
            Err(error) => ColdFileShellResult {
                path,
                result: Err(error.to_string()),
            },
        })
        .collect();
    result_tx
        .send(BackgroundIoResult::ColdFileShellsBuilt {
            request_id,
            shells,
            selected_path,
        })
        .is_err()
}

// Open multiple files concurrently to exploit SSD throughput while keeping
// memory pressure bounded. Modern NVMe SSDs benefit from a handful of
// concurrent reads; we cap the fanout to keep the bursts predictable.
const MAX_CONCURRENT_READS: usize = 8;

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
        .map_or(2, |p| p.get().min(MAX_CONCURRENT_READS))
        .min(total);
    let chunk_size = total.div_ceil(worker_count);
    let mut indexed = requests.into_iter().enumerate();
    let chunks = std::iter::from_fn(|| {
        let chunk = indexed.by_ref().take(chunk_size).collect::<Vec<_>>();
        (!chunk.is_empty()).then_some(chunk)
    });
    let (load_tx, load_rx) = mpsc::channel::<(usize, LoadedPathResult)>();

    thread::scope(|scope| {
        for chunk in chunks {
            let load_tx = load_tx.clone();
            scope.spawn(move || load_chunk(chunk, &load_tx));
        }
        drop(load_tx);
        for (index, result) in load_rx {
            consume(index, result);
        }
    });
}

/// Loads `chunk` in order, stopping early once the consumer has gone away.
fn load_chunk(
    chunk: Vec<(usize, PathLoadRequest)>,
    load_tx: &mpsc::Sender<(usize, LoadedPathResult)>,
) {
    for (index, request) in chunk {
        if load_tx.send((index, load_one(request))).is_err() {
            return;
        }
    }
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
    let mut send_failed = false;

    for_each_loaded(requests, |index, result| {
        if send_failed {
            return;
        }
        pending.insert(index, result);
        while let Some(result) = pending.remove(&next_index) {
            next_index += 1;
            let is_partial = next_index < total;
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
    load_one_with_context(request, ImportContext::default())
}

/// Wrapper requests adjust the import context or post-process the inner
/// load; leaf requests read one path.
fn load_one_with_context(request: PathLoadRequest, mut context: ImportContext) -> LoadedPathResult {
    match request {
        PathLoadRequest::Cancelable { cancelled, request } => {
            context.cancelled = Some(cancelled);
            load_one_with_context(*request, context)
        }
        PathLoadRequest::Reconcile {
            base,
            source,
            materialized_only,
            request,
        } => {
            let mut loaded = load_one_with_context(*request, context);
            loaded.result = loaded
                .result
                .and_then(|loaded| reconcile_loaded(*base, source, materialized_only, loaded));
            loaded
        }
        PathLoadRequest::Bound { store, request } => {
            context.store = Some(store.clone());
            let mut loaded = load_one_with_context(*request, context);
            bind_loaded_to_store(&mut loaded, &store);
            loaded
        }
        PathLoadRequest::Observed {
            path,
            materialize,
            observed_digest,
        } => load_observed(path, materialize, observed_digest.as_deref(), &context),
        PathLoadRequest::Retained {
            path,
            session_store,
            claimed,
        } => {
            if context.store.is_none() {
                context.store = available_chunk_store(&session_store);
            }
            load_retained(path, &session_store, &claimed, &context)
        }
        PathLoadRequest::Standard(path) => load_path_result(path, |path| {
            FileService::read_file_with_context(path, &context)
        }),
        PathLoadRequest::WithEncoding {
            path,
            encoding_name,
        } => load_path_result(path, |path| {
            FileService::read_file_with_encoding_context(path, &encoding_name, &context)
        }),
    }
}

fn reconcile_loaded(
    base: BufferState,
    source: Option<PieceSource>,
    materialized_only: bool,
    loaded: BufferState,
) -> Result<BufferState, String> {
    if materialized_only && !loaded.replacement_ready {
        return Ok(loaded);
    }
    base.prepare_loaded_replacement(loaded, source)
        .map_err(|cause| cause.to_string())
}

/// Binds every loaded document to `store`; any binding failure fails the load.
fn bind_loaded_to_store(loaded: &mut LoadedPathResult, store: &ChunkStoreHandle) {
    if let Ok(buffer) = &mut loaded.result
        && let Err(cause) = buffer.document_mut().bind_store(store.clone())
    {
        loaded.result = Err(cause.to_string());
    }
    let retained_buffers = loaded
        .retained_tab
        .iter_mut()
        .flat_map(WorkspaceTab::buffers_mut);
    for buffer in retained_buffers {
        if let Err(cause) = buffer.document_mut().bind_store(store.clone()) {
            loaded.result = Err(cause.to_string());
        }
    }
}

/// Reopens retained session state for `path`, falling back to the disk file
/// when nothing is retained or the session store cannot be written.
fn load_retained(
    path: PathBuf,
    session_store: &SessionStore,
    claimed: &[CanonicalPathKey],
    context: &ImportContext,
) -> LoadedPathResult {
    match session_store.reopen_path_with_context(&path, claimed, context) {
        Ok(Some(tab)) => LoadedPathResult {
            missing: false,
            path,
            disk_state: tab.active_buffer().disk_state.clone(),
            result: Ok(tab.active_buffer().clone()),
            retained_tab: Some(tab),
        },
        Err(error) if !session_store.writes_blocked() => LoadedPathResult::failed(path, &error),
        Ok(None) | Err(_) => load_path_result(path, |path| {
            FileService::read_file_with_context(path, context)
        }),
    }
}

fn build_cold_file_shell(path: PathBuf, store: &SessionStore) -> ColdFileShellResult {
    // Adopt the requested version before publishing a cold tab. A new source
    // is distinct from a damaged committed manifest: restore never falls back
    // to a potentially newer disk file.
    let result = store
        .chunk_store_handle()
        .and_then(|handle| {
            let content = FileService::read_file_in_store(&path, handle)?;
            let tab = WorkspaceTab::new(FileService::build_buffer_from_file_content(
                &path,
                content,
                FileService::read_disk_state(&path).ok(),
            ));
            Ok(store.cold_tab_from_imported(&tab))
        })
        .map_err(|error| error.to_string());
    ColdFileShellResult { path, result }
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
        missing: false,
        retained_tab: None,
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
    let digest = snapshot
        .try_content_digest()
        .map_err(|error| error.to_string());
    let result = digest.as_ref().map_err(Clone::clone).and_then(|_| {
        FileService::write_snapshot_with_format(&path, &snapshot, &format)
            .map_err(|error| error.to_string())
    });
    let disk_state = result
        .as_ref()
        .ok()
        .and_then(|()| FileService::read_disk_state(&path).ok());
    let digest = if result.is_ok() { digest.ok() } else { None };
    BackgroundIoResult::PathSaved {
        request_id,
        path,
        disk_state,
        digest,
        result,
    }
}

fn load_observed(
    path: PathBuf,
    materialize: bool,
    known_digest: Option<&str>,
    context: &ImportContext,
) -> LoadedPathResult {
    match FileService::read_observed_buffer_with_context(&path, materialize, known_digest, context)
    {
        Ok(buffer) => LoadedPathResult {
            path,
            disk_state: buffer.disk_state.clone(),
            result: Ok(buffer),
            missing: false,
            retained_tab: None,
        },
        Err(error) => LoadedPathResult::failed(path, &error),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackgroundIoResult, PathLoadRequest, load_one, load_paths, stream_cold_file_shells,
    };

    #[test]
    fn large_batch_delivers_selected_text_before_the_inactive_shells() {
        for selected_exists in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let store = crate::app::services::session_store::SessionStore::new(
                directory.path().join("session"),
            );
            let paths: Vec<_> = (0..3)
                .map(|index| {
                    let path = directory.path().join(format!("file-{index}.txt"));
                    if index < 2 || selected_exists {
                        std::fs::write(&path, format!("text {index}")).unwrap();
                    }
                    path
                })
                .collect();
            let (tx, rx) = std::sync::mpsc::channel();
            let tx =
                super::Sender::new(tx, crate::app::services::repaint::RepaintSignal::default());
            assert!(!stream_cold_file_shells(42, paths.clone(), &store, &tx));
            let BackgroundIoResult::PathsLoaded {
                request_id,
                results,
                is_partial,
            } = rx.recv().unwrap()
            else {
                panic!("selected file must arrive first");
            };
            assert_eq!(request_id, 42);
            assert!(is_partial);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].path, paths[2]);
            assert_eq!(results[0].result.is_ok(), selected_exists);
            if selected_exists {
                assert_eq!(results[0].result.as_ref().unwrap().text(), "text 2");
            }
            let BackgroundIoResult::ColdFileShellsBuilt {
                shells,
                selected_path,
                ..
            } = rx.recv().unwrap()
            else {
                panic!("inactive shell batch must terminate the request");
            };
            assert_eq!(selected_path.as_ref(), Some(&paths[2]));
            assert_eq!(
                shells.iter().map(|s| &s.path).collect::<Vec<_>>(),
                paths[..2].iter().collect::<Vec<_>>()
            );
            assert!(shells.iter().all(|s| s.result.is_ok()));
            assert!(rx.try_recv().is_err());
        }
    }

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
}
