use super::model::{SessionManifest, SessionTab};
use crate::app::diagnostics;
use crate::app::domain::DocumentSnapshot;
use crate::app::services::file_service::FileService;
use crate::app::services::store_io::write_atomic;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;

pub const BUFFER_FILE_EXTENSION: &str = "tmp";
pub(crate) const SESSION_IO_PARALLEL_MIN_ITEMS: usize = 512;
pub(crate) const SESSION_IO_PARALLEL_MAX_WORKERS: usize = 8;

pub(super) struct SessionSnapshotWrite {
    pub(super) temp_id: String,
    pub(super) path: PathBuf,
    pub(super) snapshot: DocumentSnapshot,
}

pub(super) struct ManifestWriteProfile {
    pub(super) serialize_ns: u128,
    pub(super) write_ns: u128,
    pub(super) size_bytes: u64,
}

pub(crate) fn collect_stale_buffer_files(
    root: &Path,
    manifest_path: &Path,
    active_temp_paths: &HashSet<PathBuf>,
) -> io::Result<Vec<PathBuf>> {
    let mut stale_paths = Vec::new();

    if !root.exists() {
        return Ok(stale_paths);
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();

        if is_stale_buffer_file(&path, manifest_path, active_temp_paths) {
            stale_paths.push(path);
        }
    }

    Ok(stale_paths)
}

pub(super) fn read_session_manifest(manifest_path: &Path) -> io::Result<SessionManifest> {
    let raw = fs::read_to_string(manifest_path).inspect_err(|error| {
        record_session_io_error(
            "session_read_manifest",
            manifest_path,
            "session_store::load_manifest",
            error,
        );
    })?;
    parse_session_manifest(manifest_path, &raw)
}

fn parse_session_manifest(manifest_path: &Path, raw: &str) -> io::Result<SessionManifest> {
    serde_json::from_str(raw).map_err(|error| {
        let error = invalid_data(error);
        record_session_io_error(
            "session_parse_manifest",
            manifest_path,
            "session_store::load_manifest",
            &error,
        );
        error
    })
}

pub(super) fn serialize_session_manifest(
    manifest_path: &Path,
    manifest: &SessionManifest,
    pretty_tab_limit: usize,
) -> io::Result<Vec<u8>> {
    let result = if manifest.tabs.len() <= pretty_tab_limit {
        serde_json::to_vec_pretty(manifest)
    } else {
        serde_json::to_vec(manifest)
    };
    result.map_err(|error| {
        let error = invalid_data(error);
        record_session_io_error(
            "session_serialize_manifest",
            manifest_path,
            "session_store::persist_request",
            &error,
        );
        error
    })
}

pub(super) fn write_session_manifest_profiled(
    manifest_path: &Path,
    manifest: &SessionManifest,
    pretty_tab_limit: usize,
) -> io::Result<ManifestWriteProfile> {
    let serialize_start = Instant::now();
    let json = serialize_session_manifest(manifest_path, manifest, pretty_tab_limit)?;
    let serialize_ns = serialize_start.elapsed().as_nanos();
    let size_bytes = json.len() as u64;

    let write_start = Instant::now();
    write_atomic(manifest_path, &json).inspect_err(|error| {
        record_session_io_error(
            "session_write_manifest",
            manifest_path,
            "session_store::persist_request",
            error,
        );
    })?;

    Ok(ManifestWriteProfile {
        serialize_ns,
        write_ns: write_start.elapsed().as_nanos(),
        size_bytes,
    })
}

fn is_stale_buffer_file(
    path: &Path,
    manifest_path: &Path,
    active_temp_paths: &HashSet<PathBuf>,
) -> bool {
    if path == manifest_path || active_temp_paths.contains(path) {
        return false;
    }

    path.extension().and_then(|ext| ext.to_str()) == Some(BUFFER_FILE_EXTENSION)
}

pub(super) fn write_session_snapshots(
    writes: Vec<SessionSnapshotWrite>,
) -> io::Result<HashSet<PathBuf>> {
    let total = writes.len();
    if total == 0 {
        return Ok(HashSet::new());
    }

    let workers = session_io_worker_count(total);
    if should_write_session_snapshots_serially(total, workers) {
        return write_session_snapshot_chunk(writes).map(|paths| paths.into_iter().collect());
    }

    write_session_snapshots_parallel(writes, total, workers)
}

fn should_write_session_snapshots_serially(total: usize, workers: usize) -> bool {
    total < SESSION_IO_PARALLEL_MIN_ITEMS || workers <= 1
}

fn write_session_snapshots_parallel(
    writes: Vec<SessionSnapshotWrite>,
    total: usize,
    workers: usize,
) -> io::Result<HashSet<PathBuf>> {
    let chunks = session_snapshot_chunks(writes, total, workers);
    thread::scope(|scope| {
        let handles = chunks
            .into_iter()
            .map(|chunk| scope.spawn(move || write_session_snapshot_chunk(chunk)))
            .collect::<Vec<_>>();

        collect_session_snapshot_results(handles, total)
    })
}

fn session_snapshot_chunks(
    writes: Vec<SessionSnapshotWrite>,
    total: usize,
    workers: usize,
) -> Vec<Vec<SessionSnapshotWrite>> {
    let chunk_size = total.div_ceil(workers);
    let mut iter = writes.into_iter();
    let mut chunks = Vec::with_capacity(workers);
    for _ in 0..workers {
        let chunk = iter.by_ref().take(chunk_size).collect::<Vec<_>>();
        if chunk.is_empty() {
            break;
        }
        chunks.push(chunk);
    }
    chunks
}

fn collect_session_snapshot_results(
    handles: Vec<thread::ScopedJoinHandle<'_, io::Result<Vec<PathBuf>>>>,
    total: usize,
) -> io::Result<HashSet<PathBuf>> {
    let mut active_temp_paths = HashSet::with_capacity(total);
    let mut first_error = None;
    for handle in handles {
        match session_snapshot_join_result(handle) {
            Ok(paths) => active_temp_paths.extend(paths),
            Err(error) => remember_first_session_snapshot_error(&mut first_error, error),
        }
    }

    match first_error {
        Some(error) => Err(error),
        None => Ok(active_temp_paths),
    }
}

fn session_snapshot_join_result(
    handle: thread::ScopedJoinHandle<'_, io::Result<Vec<PathBuf>>>,
) -> io::Result<Vec<PathBuf>> {
    match handle.join() {
        Ok(result) => result,
        Err(_) => Err(io::Error::other("session snapshot writer panicked")),
    }
}

fn remember_first_session_snapshot_error(first_error: &mut Option<io::Error>, error: io::Error) {
    if first_error.is_none() {
        *first_error = Some(error);
    }
}

fn write_session_snapshot_chunk(writes: Vec<SessionSnapshotWrite>) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::with_capacity(writes.len());
    for write in writes {
        paths.push(write_one_session_snapshot(write)?);
    }
    Ok(paths)
}

fn write_one_session_snapshot(write: SessionSnapshotWrite) -> io::Result<PathBuf> {
    FileService::write_snapshot_utf8(&write.path, &write.snapshot).inspect_err(|error| {
        diagnostics::record_io_error_with_details(
            "session_write_buffer_snapshot",
            Some(&write.path),
            "session_store::persist_request",
            error,
            [("temp_id", write.temp_id.clone())],
        );
    })?;
    Ok(write.path)
}

pub(super) fn session_tab_temp_ids(tab: &SessionTab) -> Vec<&str> {
    if !tab.buffers.is_empty() {
        return tab
            .buffers
            .iter()
            .map(|buffer| buffer.temp_id.as_str())
            .collect();
    }
    tab.temp_id.as_deref().into_iter().collect()
}

fn session_io_worker_count(item_count: usize) -> usize {
    thread::available_parallelism()
        .map_or(1, |parallelism| {
            parallelism
                .get()
                .min(SESSION_IO_PARALLEL_MAX_WORKERS)
                .min(item_count)
        })
        .max(1)
}

fn invalid_data(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

fn record_session_io_error(
    operation: &'static str,
    path: &Path,
    scope: &'static str,
    error: &io::Error,
) {
    diagnostics::record_io_error(operation, Some(path), scope, error);
}
