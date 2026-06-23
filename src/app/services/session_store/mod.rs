mod capture;
mod model;
mod ops;
mod restore;
mod types;

use crate::app::diagnostics;
use crate::app::domain::WorkspaceTab;
use crate::app::services::settings_store::AppSettings;
use crate::app::services::store_io::remove_file_if_exists;
use capture::{CapturedSessionBuffer, CapturedSessionTab};
use model::{SessionBuffer, SessionManifest, SessionPaneNode, SessionTab, SessionView};
use ops::{
    BUFFER_FILE_EXTENSION, SessionSnapshotWrite, collect_stale_buffer_files, session_tab_temp_ids,
    write_session_manifest_profiled, write_session_snapshots,
};
use restore::RestoreSummary;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Instant;
use types::{PreparedSessionPersist, RestoredTabs, StreamedTabs};

const SESSION_DIR_NAME: &str = "scratchpad";
const SESSION_MANIFEST_NAME: &str = "session.json";
const PRETTY_SESSION_MANIFEST_MAX_TABS: usize = 128;

pub(crate) use capture::{SessionPersistRequest, cold_tab_from_workspace_tab};
pub use model::SESSION_VERSION;
pub use model::SessionActiveSurface;
pub(crate) use model::SessionTabParts as ColdSessionTab;
pub(super) use ops::{SESSION_IO_PARALLEL_MAX_WORKERS, SESSION_IO_PARALLEL_MIN_ITEMS};
pub use types::{
    ProfiledRestoredSession, RestoreStatus, RestoreStatusLevel, RestoredSession,
    SessionPersistProfile, SessionRestoreProfile,
};

#[derive(Clone)]
pub struct SessionStore {
    root: PathBuf,
    manifest_path: PathBuf,
    fallback_root: Option<PathBuf>,
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
            fallback_root: None,
        }
    }

    #[must_use]
    pub fn with_fallback(root: PathBuf, fallback_root: PathBuf) -> Self {
        let mut store = Self::new(root);
        store.fallback_root = Some(fallback_root);
        store
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
        let manifest_profile = write_session_manifest_profiled(
            &self.manifest_path,
            &manifest,
            PRETTY_SESSION_MANIFEST_MAX_TABS,
        )?;
        profile.manifest_serialize_ns = manifest_profile.serialize_ns;
        profile.manifest_size_bytes = manifest_profile.size_bytes;
        profile.manifest_write_ns = manifest_profile.write_ns;
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
            self.migrate_fallback_session_if_needed()?;
        }

        if !self.manifest_path.exists() {
            return Ok(None);
        }

        let manifest = ops::read_session_manifest(&self.manifest_path)?;
        if !self.is_supported_session_manifest(&manifest) {
            return Ok(None);
        }

        Ok(Some(manifest))
    }

    fn migrate_fallback_session_if_needed(&self) -> io::Result<()> {
        let Some(fallback_root) = &self.fallback_root else {
            return Ok(());
        };
        let fallback_manifest_path = fallback_root.join(SESSION_MANIFEST_NAME);
        if !fallback_manifest_path.exists() {
            return Ok(());
        }

        fs::create_dir_all(&self.root).inspect_err(|error| {
            diagnostics::record_io_error(
                "session_create_root_for_migration",
                Some(&self.root),
                "session_store::migrate_fallback_session_if_needed",
                error,
            );
        })?;

        for entry in fs::read_dir(fallback_root)? {
            let entry = entry?;
            let source_path = entry.path();
            if !source_path.is_file() || !is_session_store_file(&source_path) {
                continue;
            }
            let destination_path = self.root.join(entry.file_name());
            if destination_path.exists() {
                continue;
            }
            fs::copy(&source_path, &destination_path).inspect_err(|error| {
                diagnostics::record_io_error(
                    "session_migrate_fallback_file",
                    Some(&source_path),
                    "session_store::migrate_fallback_session_if_needed",
                    error,
                );
            })?;
        }

        Ok(())
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

fn is_session_store_file(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == SESSION_MANIFEST_NAME)
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == BUFFER_FILE_EXTENSION)
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

#[cfg(test)]
mod migration_tests {
    use super::is_session_store_file;
    use std::path::Path;

    #[test]
    fn fallback_session_migration_copies_only_session_files() {
        assert!(is_session_store_file(Path::new("session.json")));
        assert!(is_session_store_file(Path::new("buffer.tmp")));
        assert!(!is_session_store_file(Path::new("settings.toml")));
    }
}
