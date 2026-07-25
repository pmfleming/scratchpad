use super::{LaneEndpoints, LaneOutcome, spawn_lane};
use crate::app::capacity_metrics::BackgroundIoLane;
use crate::app::domain::{DocumentSnapshot, TextFormatMetadata};
use crate::app::services::background_io::types::{
    BackgroundIoRequest, BackgroundIoResult, ColdFileShellResult, LoadedPathResult, PathLoadRequest,
};
use crate::app::services::file_service::{FileContent, FileService};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread;

pub(in crate::app::services::background_io) fn spawn_path_lane(endpoints: LaneEndpoints) {
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
            BackgroundIoRequest::BuildColdFileShells { request_id, paths } => {
                LaneOutcome::result(BackgroundIoResult::ColdFileShellsBuilt {
                    request_id,
                    shells: paths.into_iter().map(build_cold_file_shell).collect(),
                })
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

fn build_cold_file_shell(path: PathBuf) -> ColdFileShellResult {
    let result = FileService::read_disk_state(&path)
        .map(|disk_state| {
            let tab = crate::app::domain::WorkspaceTab::new(FileService::build_cold_file_shell(
                &path,
                Some(disk_state),
            ));
            let cold_tab = crate::app::services::session_store::cold_tab_from_workspace_tab(&tab);
            (tab, cold_tab)
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
        .and_then(|()| FileService::read_disk_state(&path).ok());
    BackgroundIoResult::PathSaved {
        request_id,
        path,
        disk_state,
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::{PathLoadRequest, load_one, load_paths};

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
