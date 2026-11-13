use super::PendingBackgroundAction;
use crate::app::diagnostics;
use crate::app::domain::BufferId;
use crate::app::platform_file::OpenFileDialogKind;
use crate::app::services::background_io::{BackgroundIoDispatcher, BackgroundIoResult};
use crate::app::services::file_watch::{FileWatchEvent, FileWatchService};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Weak};
use std::time::Instant;

pub(crate) struct OpenFileDialogState {
    pub(crate) kind: OpenFileDialogKind,
    pub(crate) rx: Receiver<Option<Vec<PathBuf>>>,
}

pub(crate) struct BackgroundIoState {
    pub(crate) path_imports: HashMap<PathBuf, Vec<Weak<AtomicBool>>>,
    pub(crate) bound_store_epoch: Option<u64>,
    pub(crate) preparations_pending: bool,
    pub(crate) tx: BackgroundIoDispatcher,
    pub(crate) rx: Receiver<BackgroundIoResult>,
    pub(crate) next_request_id: u64,
    pub(crate) pending_background_actions: HashMap<u64, PendingBackgroundAction>,
}

impl BackgroundIoState {
    pub(crate) fn new(tx: BackgroundIoDispatcher, rx: Receiver<BackgroundIoResult>) -> Self {
        Self {
            path_imports: HashMap::new(),
            bound_store_epoch: None,
            preparations_pending: false,
            tx,
            rx,
            next_request_id: 1,
            pending_background_actions: HashMap::new(),
        }
    }

    pub(crate) fn has_pending_background_actions(&self) -> bool {
        !self.pending_background_actions.is_empty()
    }

    pub(crate) fn allocate_background_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        request_id
    }

    pub(crate) fn insert_pending_background_action(
        &mut self,
        request_id: u64,
        action: PendingBackgroundAction,
    ) {
        self.pending_background_actions.insert(request_id, action);
    }

    pub(crate) fn remove_pending_background_action(
        &mut self,
        request_id: u64,
    ) -> Option<PendingBackgroundAction> {
        self.pending_background_actions.remove(&request_id)
    }

    pub(crate) fn pending_background_action_mut(
        &mut self,
        request_id: u64,
    ) -> Option<&mut PendingBackgroundAction> {
        self.pending_background_actions.get_mut(&request_id)
    }

    fn any_pending(&self, matches: impl Fn(&PendingBackgroundAction) -> bool) -> bool {
        self.pending_background_actions.values().any(matches)
    }

    pub(crate) fn has_pending_persist(&self) -> bool {
        self.any_pending(|action| matches!(action, PendingBackgroundAction::PersistSession(_)))
    }

    pub(crate) fn has_pending_session_hydration(&self) -> bool {
        self.any_pending(|action| matches!(action, PendingBackgroundAction::HydrateSessionTab(_)))
    }

    pub(crate) fn has_pending_retained_open(&self, document: &str) -> bool {
        self.any_pending(|action| {
            matches!(action, PendingBackgroundAction::ReopenRetained(open)
                if open.document.as_deref() == Some(document))
        })
    }

    pub(crate) fn has_pending_save_for_buffer(&self, buffer_id: BufferId) -> bool {
        self.any_pending(|action| {
            matches!(action, PendingBackgroundAction::SavePath(save) if save.buffer_id == buffer_id)
        })
    }

    pub(crate) fn has_pending_reload_for_buffer(&self, buffer_id: BufferId) -> bool {
        self.any_pending(|action| {
            matches!(action, PendingBackgroundAction::ReloadBuffer(reload)
                if reload.buffer_id == buffer_id)
        })
    }

    pub(crate) fn defer_save_until_observed(
        &mut self,
        buffer_id: BufferId,
        save: super::DeferredSave,
    ) -> bool {
        for action in self.pending_background_actions.values_mut() {
            if let PendingBackgroundAction::ReloadBuffer(reload) = action
                && reload.buffer_id == buffer_id
                && reload.mode == super::PendingReloadMode::ObserveDiskChange
            {
                reload.resume_save = Some(save);
                return true;
            }
        }
        false
    }

    pub(crate) fn has_pending_reopen_with_encoding_for_buffer(&self, buffer_id: BufferId) -> bool {
        self.any_pending(|action| {
            matches!(action, PendingBackgroundAction::ReopenWithEncoding(reopen)
                if reopen.buffer_id == buffer_id)
        })
    }

    pub(crate) fn has_pending_text_metadata_refresh(
        &self,
        buffer_id: BufferId,
        revision: u64,
    ) -> bool {
        self.any_pending(|action| {
            matches!(action, PendingBackgroundAction::RefreshTextMetadata(pending)
                if pending.buffer_id == buffer_id && pending.revision == revision)
        })
    }

    pub(crate) fn has_pending_encoding_compliance_refresh(
        &self,
        buffer_id: BufferId,
        revision: u64,
    ) -> bool {
        self.any_pending(|action| {
            matches!(action, PendingBackgroundAction::RefreshEncodingCompliance(pending)
                if pending.buffer_id == buffer_id && pending.revision == revision)
        })
    }

    /// Registers a cancellation token for a new background import of `path`.
    pub(crate) fn register_path_import(&mut self, path: &Path) -> Arc<AtomicBool> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let tokens = self.path_imports.entry(path.to_path_buf()).or_default();
        tokens.retain(|token| token.strong_count() > 0);
        tokens.push(Arc::downgrade(&cancelled));
        cancelled
    }

    /// Forgets tokens whose imports of `path` have finished.
    pub(crate) fn prune_path_imports(&mut self, path: &Path) {
        if let Some(tokens) = self.path_imports.get_mut(path) {
            tokens.retain(|token| token.strong_count() > 0);
            if tokens.is_empty() {
                self.path_imports.remove(path);
            }
        }
    }

    /// Asks every in-flight import of `path` to stop.
    pub(crate) fn cancel_path_imports(&mut self, path: &Path) {
        for token in self.path_imports.remove(path).into_iter().flatten() {
            if let Some(token) = token.upgrade() {
                token.store(true, Ordering::Release);
            }
        }
    }
}

pub(crate) struct FileWatchState {
    pub(super) directories: super::file_watch::WatchDirectories,
    pub(crate) file_watch_service: FileWatchService,
    pub(crate) pending_file_watch_rescans: HashMap<PathBuf, Instant>,
}

impl Default for FileWatchState {
    fn default() -> Self {
        Self {
            directories: super::file_watch::WatchDirectories::default(),
            file_watch_service: FileWatchService::new(),
            pending_file_watch_rescans: HashMap::new(),
        }
    }
}

impl FileWatchState {
    pub(crate) fn set_watched_file_directories(&mut self, dirs: BTreeSet<PathBuf>) {
        self.file_watch_service.set_watched_directories(dirs);
    }

    pub(crate) fn collect_file_watch_events(&mut self, due_at: Instant) {
        for event in self.file_watch_service.drain_events() {
            match event {
                FileWatchEvent::DirectoryChanged(dir) => {
                    self.pending_file_watch_rescans.insert(dir, due_at);
                }
                FileWatchEvent::WatchError { path, message } => {
                    self.directories.invalidate();
                    diagnostics::record_warning(
                        "file_watch_unavailable",
                        path.as_deref(),
                        "app_state::file_watch",
                        message,
                    );
                }
            }
        }
    }

    pub(crate) fn take_due_file_watch_rescans(&mut self, now: Instant) -> Vec<PathBuf> {
        let due_dirs = self
            .pending_file_watch_rescans
            .iter()
            .filter_map(|(dir, due_at)| (*due_at <= now).then_some(dir.clone()))
            .collect::<Vec<_>>();

        for dir in &due_dirs {
            self.pending_file_watch_rescans.remove(dir);
        }

        due_dirs
    }

    pub(crate) fn has_pending_file_watch_rescans(&self) -> bool {
        !self.pending_file_watch_rescans.is_empty()
    }
}
