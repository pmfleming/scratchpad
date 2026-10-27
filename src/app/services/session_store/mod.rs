mod capture;
mod database;
mod model;
mod parallel;
#[cfg(test)]
mod parity_tests;
mod recovery_export;
mod restore;
mod types;
mod usage;
pub use usage::StorageUsage;
mod validation;

use crate::app::diagnostics;
use crate::app::domain::WorkspaceTab;
use crate::app::services::settings_store::AppSettings;
use database::{DatabaseWorker, ReadScope, StoredSession};
use futures_lite::future::block_on;
use model::{SessionBuffer, SessionManifest, SessionPaneNode, SessionTab, SessionView};
use restore::Restorer;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

pub(crate) use capture::{CapturedClosedTab, SessionPersistRequest, cold_tab_from_workspace_tab};
pub use database::versions::BufferVersionInfo;
pub use retained::ClosedTabInfo;
mod reopen;
mod retained;
pub(crate) use model::SessionTabParts as ColdSessionTab;
pub use model::{SESSION_VERSION, SessionActiveSurface};
pub use types::{
    ProfiledRestoredSession, RestoreStatus, RestoreStatusLevel, RestoredSession,
    SessionPersistProfile, SessionRestoreProfile,
};

/// One lazy database worker per store; clones share ownership and commit ordering.
#[derive(Clone)]
pub struct SessionStore {
    root: PathBuf,
    worker: Arc<OnceLock<Result<DatabaseWorker, String>>>,
}
impl Default for SessionStore {
    fn default() -> Self {
        Self::new(std::env::temp_dir().join("scratchpad"))
    }
}
impl SessionStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            worker: Arc::new(OnceLock::new()),
        }
    }
    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    fn worker(&self) -> io::Result<&DatabaseWorker> {
        self.worker
            .get_or_init(|| {
                DatabaseWorker::open(self.root.clone()).map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|message| io::Error::other(message.clone()))
    }
    /// A protection latch or cached worker-open failure cannot be retried by this
    /// store. Ordinary transaction errors remain retryable and must not discard edits.
    pub(crate) fn writes_blocked(&self) -> bool {
        match self.worker() {
            Ok(worker) => worker.call(|db| Ok(db.blocked.is_some())).unwrap_or(true),
            Err(_) => true,
        }
    }
    fn read_session(&self, scope: ReadScope) -> io::Result<Option<StoredSession>> {
        self.worker()?
            .call(move |db| block_on(db.load(scope)))
            .inspect_err(|error| {
                diagnostics::record_io_error(
                    "session_database_read",
                    Some(&self.root),
                    "session_store",
                    error,
                );
            })
    }
    pub fn load(&self) -> io::Result<Option<RestoredSession>> {
        self.load_profiled().map(|result| result.restored)
    }
    pub fn load_profiled(&self) -> io::Result<ProfiledRestoredSession> {
        let start = Instant::now();
        let stored = self.read_session(ReadScope::All)?;
        let mut profile = SessionRestoreProfile {
            manifest_read_parse_ns: start.elapsed().as_nanos(),
            ..SessionRestoreProfile::default()
        };
        let restored = stored
            .map(|stored| {
                profile.tab_count = stored.manifest.tabs.len();
                profile.buffer_count = stored
                    .manifest
                    .tabs
                    .iter()
                    .map(SessionTab::buffer_count)
                    .sum();
                let reconstruct = Instant::now();
                let restored = self.restore_guarded(stored);
                profile.restore_reconstruction_ns = reconstruct.elapsed().as_nanos();
                restored
            })
            .transpose()?
            .flatten();
        profile.total_ns = start.elapsed().as_nanos();
        Ok(ProfiledRestoredSession { restored, profile })
    }
    pub fn load_startup_visible(&self) -> io::Result<Option<RestoredSession>> {
        self.read_session(ReadScope::Active)?
            .map(|stored| self.restore_guarded(stored))
            .transpose()
            .map(Option::flatten)
    }
    fn restore_guarded(&self, stored: StoredSession) -> io::Result<Option<RestoredSession>> {
        // Content failures are quarantined by the database before reaching here.
        // An unscoped worker spawn/panic failure still protects the whole manifest.
        restore_session(stored).inspect_err(|error| {
            let reason = error.to_string();
            if let Ok(worker) = self.worker() {
                let _ = worker.call(move |db| {
                    db.blocked = Some(reason);
                    Ok(())
                });
            }
        })
    }
    pub(crate) fn load_streaming(
        &self,
        mut on_started: impl FnMut(usize, SessionActiveSurface, AppSettings) -> bool,
        mut on_tab: impl FnMut(usize, WorkspaceTab, Option<ColdSessionTab>) -> bool,
    ) -> io::Result<Option<RestoredSession>> {
        let Some(stored) = self.read_session(ReadScope::Shells)? else {
            return Ok(None);
        };
        let mut session = restored_metadata(&stored.manifest);
        if !on_started(
            session.active_tab_index,
            session.active_surface,
            session.legacy_settings.clone(),
        ) {
            return Err(receiver_closed());
        }
        let mut restorer = Restorer::new(stored.contents);
        let tabs = restorer.restore_tabs_active_first(
            stored.manifest.tabs,
            session.active_tab_index,
            &stored.quarantined,
        );
        if tabs.is_empty() {
            return Ok(None);
        }
        let mut summary = restore::RestoreSummary::default();
        for (index, tab, cold_tab, tab_summary) in tabs {
            summary.merge(tab_summary);
            if !on_tab(index, tab, cold_tab) {
                return Err(receiver_closed());
            }
        }
        session.restore_status = if stored.quarantined.is_empty() {
            summary.into_status()
        } else {
            let prefix = summary
                .into_status()
                .map(|status| format!("{} ", status.message))
                .unwrap_or_default();
            Some(RestoreStatus {
                level: RestoreStatusLevel::Warning,
                message: format!(
                    "{prefix}{} damaged recovery buffer(s) quarantined; healthy buffers still save. Original data is retained in session.db.",
                    stored.quarantined.len()
                ),
            })
        };
        Ok(Some(session))
    }
    pub(crate) fn restore_cold_session_tab(
        &self,
        tab: ColdSessionTab,
    ) -> io::Result<(WorkspaceTab, Option<RestoreStatus>)> {
        let (tab, contents) = self
            .worker()?
            .call(move |db| block_on(db.hydrate(SessionTab::from(tab))))?;
        let (tab, summary) = Restorer::new(contents).restore_tab_with_summary(tab);
        Ok((tab, summary.into_status()))
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
        let start = Instant::now();
        let request = SessionPersistRequest::capture(tabs, active_tab_index, font_size, word_wrap);
        let capture_ns = start.elapsed().as_nanos();
        let mut profile = self.persist_profiled_request(request)?;
        profile.snapshot_capture_ns = capture_ns;
        profile.total_ns = start.elapsed().as_nanos();
        Ok(profile)
    }
    pub(crate) fn persist_request(&self, request: SessionPersistRequest) -> io::Result<()> {
        self.persist_profiled_request(request).map(|_| ())
    }
    fn persist_profiled_request(
        &self,
        request: SessionPersistRequest,
    ) -> io::Result<SessionPersistProfile> {
        self.worker()?
            .call(move |db| {
                let generation = request.generation;
                block_on(db.persist(request, generation))
            })
            .inspect_err(|error| {
                diagnostics::record_io_error(
                    "session_database_commit",
                    Some(&self.root),
                    "session_store",
                    error,
                );
            })
    }
}
fn restore_session(stored: StoredSession) -> io::Result<Option<RestoredSession>> {
    let mut session = restored_metadata(&stored.manifest);
    let mut summary = restore::RestoreSummary::default();
    for (tab, tab_summary) in
        Restorer::new(stored.contents).restore_tabs_ordered(stored.manifest.tabs)?
    {
        summary.merge(tab_summary);
        session.tabs.push(tab);
    }
    session.restore_status = summary.into_status();
    Ok((!session.tabs.is_empty()).then_some(session))
}
fn restored_metadata(manifest: &SessionManifest) -> RestoredSession {
    RestoredSession {
        tabs: Vec::new(),
        active_tab_index: manifest
            .active_tab_index
            .min(manifest.tabs.len().saturating_sub(1)),
        active_surface: manifest.active_surface,
        legacy_settings: manifest.legacy_settings(),
        restore_status: None,
    }
}
fn receiver_closed() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "session restore receiver closed")
}
