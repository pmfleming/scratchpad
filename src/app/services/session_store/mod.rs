mod capture;
mod model;
mod ops;
mod restore;

use crate::app::diagnostics;
use crate::app::domain::{DocumentSnapshot, WorkspaceTab};
use crate::app::services::file_service::FileService;
use crate::app::services::settings_store::AppSettings;
use crate::app::services::store_io::{remove_file_if_exists, write_atomic};
use model::{SessionBuffer, SessionManifest, SessionPaneNode, SessionTab, SessionView};
use ops::{BUFFER_FILE_EXTENSION, collect_stale_buffer_files};
use restore::RestoreSummary;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::thread;
use std::time::Instant;

const SESSION_DIR_NAME: &str = "scratchpad";
const SESSION_MANIFEST_NAME: &str = "session.json";
const SESSION_IO_PARALLEL_MIN_ITEMS: usize = 512;
const SESSION_IO_PARALLEL_MAX_WORKERS: usize = 8;
const PRETTY_SESSION_MANIFEST_MAX_TABS: usize = 128;

pub(crate) use capture::cold_tab_from_workspace_tab;
pub use model::SESSION_VERSION;
pub use model::SessionActiveSurface;
pub(crate) use model::SessionTabParts as ColdSessionTab;

#[derive(Clone)]
pub struct SessionStore {
    root: PathBuf,
    manifest_path: PathBuf,
}

pub(crate) struct SessionPersistRequest {
    active_tab_index: usize,
    active_surface: SessionActiveSurface,
    font_size: f32,
    word_wrap: bool,
    tabs: Vec<CapturedSessionTab>,
}

struct CapturedSessionTab {
    session_tab: SessionTab,
    buffer_snapshots: Vec<CapturedSessionBuffer>,
}

struct CapturedSessionBuffer {
    temp_id: String,
    snapshot: DocumentSnapshot,
}

struct SessionSnapshotWrite {
    temp_id: String,
    path: PathBuf,
    snapshot: DocumentSnapshot,
}

struct PreparedSessionPersist {
    session_tabs: Vec<SessionTab>,
    snapshot_writes: Vec<SessionSnapshotWrite>,
    preserved_snapshot_paths: HashSet<PathBuf>,
}

struct RestoredTabs {
    tabs: Vec<WorkspaceTab>,
    summary: RestoreSummary,
}

struct StreamedTabs {
    tab_count: usize,
    summary: RestoreSummary,
}

pub struct RestoredSession {
    pub tabs: Vec<WorkspaceTab>,
    pub active_tab_index: usize,
    pub active_surface: SessionActiveSurface,
    pub legacy_settings: AppSettings,
    pub restore_status: Option<RestoreStatus>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SessionPersistProfile {
    pub total_ns: u128,
    pub snapshot_capture_ns: u128,
    pub snapshot_write_ns: u128,
    pub stale_cleanup_ns: u128,
    pub manifest_serialize_ns: u128,
    pub manifest_write_ns: u128,
    pub tab_count: usize,
    pub buffer_count: usize,
    pub manifest_size_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SessionRestoreProfile {
    pub total_ns: u128,
    pub manifest_read_parse_ns: u128,
    pub restore_reconstruction_ns: u128,
    pub tab_count: usize,
    pub buffer_count: usize,
}

pub struct ProfiledRestoredSession {
    pub restored: Option<RestoredSession>,
    pub profile: SessionRestoreProfile,
}

#[derive(Clone)]
pub struct RestoreStatus {
    pub level: RestoreStatusLevel,
    pub message: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RestoreStatusLevel {
    Info,
    Warning,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(std::env::temp_dir().join(SESSION_DIR_NAME))
    }
}

impl SessionStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        let manifest_path = root.join(SESSION_MANIFEST_NAME);
        Self {
            root,
            manifest_path,
        }
    }

    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub fn load(&self) -> io::Result<Option<RestoredSession>> {
        self.load_profiled().map(|profiled| profiled.restored)
    }

    pub fn load_profiled(&self) -> io::Result<ProfiledRestoredSession> {
        let total_start = Instant::now();
        let manifest_start = Instant::now();
        let Some(manifest) = self.load_manifest()? else {
            return Ok(profiled_restore_none(
                total_start,
                manifest_start.elapsed().as_nanos(),
                0,
                0,
            ));
        };
        let manifest_read_parse_ns = manifest_start.elapsed().as_nanos();
        let buffer_count = manifest.tabs.iter().map(SessionTab::buffer_count).sum();
        let legacy_settings = manifest.legacy_settings();
        let active_tab_index = manifest.active_tab_index;
        let active_surface = manifest.active_surface;

        let restore_start = Instant::now();
        let RestoredTabs {
            tabs,
            summary: restore_summary,
        } = self.restore_tabs_with_summary(manifest.tabs);
        let restore_reconstruction_ns = restore_start.elapsed().as_nanos();

        if tabs.is_empty() {
            return Ok(profiled_restore_none(
                total_start,
                manifest_read_parse_ns,
                restore_reconstruction_ns,
                buffer_count,
            ));
        }

        let tab_count = tabs.len();
        Ok(ProfiledRestoredSession {
            restored: Some(RestoredSession {
                active_tab_index: active_tab_index.min(tabs.len() - 1),
                active_surface,
                tabs,
                legacy_settings,
                restore_status: restore_summary.into_status(),
            }),
            profile: SessionRestoreProfile {
                total_ns: total_start.elapsed().as_nanos(),
                manifest_read_parse_ns,
                restore_reconstruction_ns,
                tab_count,
                buffer_count,
            },
        })
    }

    fn restore_tabs_with_summary(&self, session_tabs: Vec<SessionTab>) -> RestoredTabs {
        let mut summary = RestoreSummary::default();
        let mut tabs = Vec::with_capacity(session_tabs.len());
        for (tab, tab_summary) in self.restore_tabs_ordered(session_tabs) {
            summary.merge(tab_summary);
            tabs.push(tab);
        }
        RestoredTabs { tabs, summary }
    }

    pub(crate) fn load_streaming(
        &self,
        mut on_started: impl FnMut(usize, SessionActiveSurface, AppSettings) -> bool,
        mut on_tab: impl FnMut(usize, WorkspaceTab, Option<ColdSessionTab>) -> bool,
    ) -> io::Result<Option<RestoredSession>> {
        let Some(manifest) = self.load_manifest()? else {
            return Ok(None);
        };
        let active_tab_index = manifest
            .active_tab_index
            .min(manifest.tabs.len().saturating_sub(1));
        let legacy_settings = manifest.legacy_settings();
        let active_surface = manifest.active_surface;
        notify_session_restore_started(
            &mut on_started,
            active_tab_index,
            active_surface,
            legacy_settings.clone(),
        )?;

        let StreamedTabs {
            tab_count,
            summary: restore_summary,
        } = self.stream_tabs_active_first(manifest.tabs, active_tab_index, &mut on_tab)?;

        if tab_count == 0 {
            return Ok(None);
        }

        Ok(Some(RestoredSession {
            active_tab_index: active_tab_index.min(tab_count - 1),
            active_surface,
            tabs: Vec::new(),
            legacy_settings,
            restore_status: restore_summary.into_status(),
        }))
    }

    fn stream_tabs_active_first(
        &self,
        session_tabs: Vec<SessionTab>,
        active_tab_index: usize,
        on_tab: &mut impl FnMut(usize, WorkspaceTab, Option<ColdSessionTab>) -> bool,
    ) -> io::Result<StreamedTabs> {
        let mut summary = RestoreSummary::default();
        let mut tab_count = 0usize;
        for (tab_index, restored_tab, cold_tab, tab_summary) in
            self.restore_tabs_active_first(session_tabs, active_tab_index)
        {
            summary.merge(tab_summary);
            tab_count += 1;
            if !on_tab(tab_index, restored_tab, cold_tab) {
                return Err(session_restore_receiver_closed());
            }
        }
        Ok(StreamedTabs { tab_count, summary })
    }

    pub fn load_startup_visible(&self) -> io::Result<Option<RestoredSession>> {
        let Some(mut manifest) = self.load_manifest()? else {
            return Ok(None);
        };
        if manifest.tabs.is_empty() {
            return Ok(None);
        }

        let active_tab_index = manifest
            .active_tab_index
            .min(manifest.tabs.len().saturating_sub(1));
        let active_tab = manifest.tabs.remove(active_tab_index);
        let legacy_settings = manifest.legacy_settings();
        let (tab, summary) = self.restore_tab_with_summary(active_tab);
        Ok(Some(RestoredSession {
            tabs: vec![tab],
            active_tab_index: 0,
            active_surface: manifest.active_surface,
            legacy_settings,
            restore_status: summary.into_status(),
        }))
    }

    pub fn persist(
        &self,
        tabs: &[WorkspaceTab],
        active_tab_index: usize,
        font_size: f32,
        word_wrap: bool,
    ) -> io::Result<()> {
        self.persist_profiled(tabs, active_tab_index, font_size, word_wrap)
            .map(|_| ())
    }

    pub fn persist_profiled(
        &self,
        tabs: &[WorkspaceTab],
        active_tab_index: usize,
        font_size: f32,
        word_wrap: bool,
    ) -> io::Result<SessionPersistProfile> {
        let total_start = Instant::now();
        let capture_start = Instant::now();
        let request = SessionPersistRequest::capture(tabs, active_tab_index, font_size, word_wrap);
        let mut profile = SessionPersistProfile {
            snapshot_capture_ns: capture_start.elapsed().as_nanos(),
            tab_count: request.tabs.len(),
            buffer_count: request
                .tabs
                .iter()
                .map(|tab| tab.buffer_snapshots.len())
                .sum(),
            ..SessionPersistProfile::default()
        };
        self.persist_request_profiled(request, &mut profile)?;
        profile.total_ns = total_start.elapsed().as_nanos();
        Ok(profile)
    }

    pub(crate) fn persist_request(&self, request: SessionPersistRequest) -> io::Result<()> {
        let mut profile = SessionPersistProfile::default();
        self.persist_request_profiled(request, &mut profile)
    }

    fn persist_request_profiled(
        &self,
        request: SessionPersistRequest,
        profile: &mut SessionPersistProfile,
    ) -> io::Result<()> {
        let SessionPersistRequest {
            active_tab_index,
            active_surface,
            font_size,
            word_wrap,
            tabs,
        } = request;

        fs::create_dir_all(&self.root).inspect_err(|error| {
            diagnostics::record_io_error(
                "session_create_root",
                Some(&self.root),
                "session_store::persist_request",
                &error,
            );
        })?;

        let PreparedSessionPersist {
            session_tabs,
            snapshot_writes,
            preserved_snapshot_paths,
        } = self.prepare_session_persist(tabs);

        let active_temp_paths = self.write_active_session_snapshots(
            snapshot_writes,
            preserved_snapshot_paths,
            profile,
        )?;
        self.remove_stale_buffer_files_profiled(&active_temp_paths, profile)?;

        let manifest = session_manifest(
            active_tab_index,
            active_surface,
            font_size,
            word_wrap,
            session_tabs,
        );
        self.write_manifest_profiled(&manifest, profile)?;
        Ok(())
    }

    fn prepare_session_persist(&self, tabs: Vec<CapturedSessionTab>) -> PreparedSessionPersist {
        let mut session_tabs = Vec::with_capacity(tabs.len());
        let mut snapshot_writes = Vec::new();
        let mut preserved_snapshot_paths = HashSet::new();

        for captured_tab in tabs {
            preserved_snapshot_paths.extend(
                session_tab_temp_ids(&captured_tab.session_tab)
                    .into_iter()
                    .map(|id| self.buffer_path(id)),
            );
            snapshot_writes.extend(
                captured_tab
                    .buffer_snapshots
                    .into_iter()
                    .map(|buffer| self.session_snapshot_write(buffer)),
            );
            session_tabs.push(captured_tab.session_tab);
        }

        PreparedSessionPersist {
            session_tabs,
            snapshot_writes,
            preserved_snapshot_paths,
        }
    }

    fn session_snapshot_write(&self, buffer: CapturedSessionBuffer) -> SessionSnapshotWrite {
        SessionSnapshotWrite {
            path: self.buffer_path(&buffer.temp_id),
            temp_id: buffer.temp_id,
            snapshot: buffer.snapshot,
        }
    }

    fn write_active_session_snapshots(
        &self,
        snapshot_writes: Vec<SessionSnapshotWrite>,
        preserved_snapshot_paths: HashSet<PathBuf>,
        profile: &mut SessionPersistProfile,
    ) -> io::Result<HashSet<PathBuf>> {
        let snapshot_write_start = Instant::now();
        let mut active_temp_paths = write_session_snapshots(snapshot_writes)?;
        active_temp_paths.extend(preserved_snapshot_paths);
        profile.snapshot_write_ns = snapshot_write_start.elapsed().as_nanos();
        Ok(active_temp_paths)
    }

    fn remove_stale_buffer_files_profiled(
        &self,
        active_temp_paths: &HashSet<PathBuf>,
        profile: &mut SessionPersistProfile,
    ) -> io::Result<()> {
        let stale_cleanup_start = Instant::now();
        self.remove_stale_buffer_files(active_temp_paths)?;
        profile.stale_cleanup_ns = stale_cleanup_start.elapsed().as_nanos();
        Ok(())
    }

    fn write_manifest_profiled(
        &self,
        manifest: &SessionManifest,
        profile: &mut SessionPersistProfile,
    ) -> io::Result<()> {
        let serialize_start = Instant::now();
        let json = self.serialize_manifest(manifest)?;
        profile.manifest_serialize_ns = serialize_start.elapsed().as_nanos();
        profile.manifest_size_bytes = json.len() as u64;

        let manifest_write_start = Instant::now();
        write_atomic(&self.manifest_path, &json).inspect_err(|error| {
            record_session_io_error(
                "session_write_manifest",
                &self.manifest_path,
                "session_store::persist_request",
                error,
            );
        })?;
        profile.manifest_write_ns = manifest_write_start.elapsed().as_nanos();
        Ok(())
    }

    fn serialize_manifest(&self, manifest: &SessionManifest) -> io::Result<Vec<u8>> {
        let result = if manifest.tabs.len() <= PRETTY_SESSION_MANIFEST_MAX_TABS {
            serde_json::to_vec_pretty(manifest)
        } else {
            serde_json::to_vec(manifest)
        };
        result.map_err(|error| {
            let error = invalid_data(error);
            record_session_io_error(
                "session_serialize_manifest",
                &self.manifest_path,
                "session_store::persist_request",
                &error,
            );
            error
        })
    }

    fn remove_stale_buffer_files(&self, active_temp_paths: &HashSet<PathBuf>) -> io::Result<()> {
        let stale_paths =
            collect_stale_buffer_files(&self.root, &self.manifest_path, active_temp_paths)
                .inspect_err(|error| {
                    diagnostics::record_io_error(
                        "session_collect_stale_buffers",
                        Some(&self.root),
                        "session_store::remove_stale_buffer_files",
                        &error,
                    );
                })?;

        for path in stale_paths {
            remove_file_if_exists(&path).inspect_err(|error| {
                diagnostics::record_io_error(
                    "session_remove_stale_buffer",
                    Some(&path),
                    "session_store::remove_stale_buffer_files",
                    &error,
                );
            })?;
        }

        Ok(())
    }

    fn buffer_path(&self, temp_id: &str) -> PathBuf {
        self.root.join(format!("{temp_id}.{BUFFER_FILE_EXTENSION}"))
    }

    fn load_manifest(&self) -> io::Result<Option<SessionManifest>> {
        if !self.manifest_path.exists() {
            return Ok(None);
        }

        let manifest = self.read_session_manifest()?;
        if !self.is_supported_session_manifest(&manifest) {
            return Ok(None);
        }

        Ok(Some(manifest))
    }

    fn read_session_manifest(&self) -> io::Result<SessionManifest> {
        let raw = fs::read_to_string(&self.manifest_path).inspect_err(|error| {
            record_session_io_error(
                "session_read_manifest",
                &self.manifest_path,
                "session_store::load_manifest",
                error,
            );
        })?;
        self.parse_session_manifest(&raw)
    }

    fn parse_session_manifest(&self, raw: &str) -> io::Result<SessionManifest> {
        serde_json::from_str(raw).map_err(|error| {
            let error = invalid_data(error);
            record_session_io_error(
                "session_parse_manifest",
                &self.manifest_path,
                "session_store::load_manifest",
                &error,
            );
            error
        })
    }

    fn is_supported_session_manifest(&self, manifest: &SessionManifest) -> bool {
        if manifest.version != model::SESSION_VERSION {
            diagnostics::record_warning(
                "session_version_mismatch",
                Some(&self.manifest_path),
                "session_store::load_manifest",
                format!(
                    "Session manifest version {} is not supported by version {}.",
                    manifest.version,
                    model::SESSION_VERSION
                ),
            );
            return false;
        }

        true
    }
}

fn notify_session_restore_started(
    on_started: &mut impl FnMut(usize, SessionActiveSurface, AppSettings) -> bool,
    active_tab_index: usize,
    active_surface: SessionActiveSurface,
    legacy_settings: AppSettings,
) -> io::Result<()> {
    on_started(active_tab_index, active_surface, legacy_settings)
        .then_some(())
        .ok_or_else(session_restore_receiver_closed)
}

fn session_restore_receiver_closed() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "session restore receiver closed")
}

fn profiled_restore_none(
    total_start: Instant,
    manifest_read_parse_ns: u128,
    restore_reconstruction_ns: u128,
    buffer_count: usize,
) -> ProfiledRestoredSession {
    ProfiledRestoredSession {
        restored: None,
        profile: SessionRestoreProfile {
            total_ns: total_start.elapsed().as_nanos(),
            manifest_read_parse_ns,
            restore_reconstruction_ns,
            buffer_count,
            ..SessionRestoreProfile::default()
        },
    }
}

fn session_manifest(
    active_tab_index: usize,
    active_surface: SessionActiveSurface,
    font_size: f32,
    word_wrap: bool,
    tabs: Vec<SessionTab>,
) -> SessionManifest {
    SessionManifest {
        version: SESSION_VERSION,
        active_tab_index: active_tab_index.min(tabs.len().saturating_sub(1)),
        active_surface,
        font_size,
        word_wrap,
        tabs,
    }
}

fn write_session_snapshots(writes: Vec<SessionSnapshotWrite>) -> io::Result<HashSet<PathBuf>> {
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

fn session_tab_temp_ids(tab: &SessionTab) -> Vec<&str> {
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
    path: &std::path::Path,
    scope: &'static str,
    error: &io::Error,
) {
    diagnostics::record_io_error(operation, Some(path), scope, error);
}
