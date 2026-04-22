use super::{PendingBackgroundAction, PendingStartupRestoreAction, ScratchpadApp};
use crate::app::services::background_io::{
    BackgroundIoRequest, BackgroundIoResult, LoadedPathResult, PathLoadRequest,
};
use crate::app::services::file_controller::FileController;
use crate::app::services::session_manager;
use crate::app::startup::StartupOptions;
use eframe::egui;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const BACKGROUND_IO_POLL_INTERVAL: Duration = Duration::from_millis(16);

impl ScratchpadApp {
    pub(crate) fn poll_background_io(&mut self, ctx: &egui::Context) {
        self.drain_background_io_results();

        if !self.pending_background_actions.is_empty() {
            ctx.request_repaint_after(BACKGROUND_IO_POLL_INTERVAL);
        }
    }

    pub fn drain_background_io_results(&mut self) {
        while let Ok(result) = self.background_io_rx.try_recv() {
            self.apply_background_io_result(result);
        }
    }

    pub fn wait_for_background_io_idle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            self.drain_background_io_results();
            if self.pending_background_actions.is_empty() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.drain_background_io_results();
    }

    pub(crate) fn queue_background_path_loads(
        &mut self,
        paths: Vec<PathBuf>,
        action: PendingBackgroundAction,
    ) {
        if paths.is_empty() {
            return;
        }

        let request_id = self.allocate_background_request_id();
        self.pending_background_actions.insert(request_id, action);

        let request = BackgroundIoRequest::LoadPaths {
            request_id,
            requests: paths.into_iter().map(PathLoadRequest::Standard).collect(),
        };
        if let Err(error) = self.background_io_tx.send(request) {
            self.pending_background_actions.remove(&request_id);
            self.apply_background_io_result(BackgroundIoResult::PathsLoaded {
                request_id,
                results: error.0.into_loaded_path_results().unwrap_or_default(),
            });
        }
    }

    pub(crate) fn queue_background_path_load_with_encoding(
        &mut self,
        path: PathBuf,
        encoding_name: String,
        action: PendingBackgroundAction,
    ) {
        let request_id = self.allocate_background_request_id();
        self.pending_background_actions.insert(request_id, action);

        let request = BackgroundIoRequest::LoadPaths {
            request_id,
            requests: vec![PathLoadRequest::WithEncoding {
                path,
                encoding_name,
            }],
        };
        if let Err(error) = self.background_io_tx.send(request) {
            self.pending_background_actions.remove(&request_id);
            self.apply_background_io_result(BackgroundIoResult::PathsLoaded {
                request_id,
                results: error.0.into_loaded_path_results().unwrap_or_default(),
            });
        }
    }

    pub(crate) fn queue_background_session_restore(
        &mut self,
        startup_options: StartupOptions,
        loaded_from_settings: bool,
    ) {
        let request_id = self.allocate_background_request_id();
        self.pending_background_actions.insert(
            request_id,
            PendingBackgroundAction::StartupRestore(PendingStartupRestoreAction {
                startup_options,
                loaded_from_settings,
            }),
        );

        let request = BackgroundIoRequest::RestoreSession {
            request_id,
            session_store: self.session_store.clone(),
        };
        if let Err(error) = self.background_io_tx.send(request) {
            self.pending_background_actions.remove(&request_id);
            self.apply_background_io_result(BackgroundIoResult::SessionRestored {
                request_id,
                result: error.0.into_restore_result(),
            });
        }
    }

    fn allocate_background_request_id(&mut self) -> u64 {
        let request_id = self.next_background_request_id;
        self.next_background_request_id = self.next_background_request_id.saturating_add(1);
        request_id
    }

    fn apply_background_io_result(&mut self, result: BackgroundIoResult) {
        match result {
            BackgroundIoResult::PathsLoaded {
                request_id,
                results,
            } => match self.pending_background_actions.remove(&request_id) {
                Some(PendingBackgroundAction::OpenTabs(action)) => {
                    FileController::apply_async_open_tabs_result(self, action, results);
                }
                Some(PendingBackgroundAction::OpenHere(action)) => {
                    FileController::apply_async_open_here_result(self, action, results);
                }
                Some(PendingBackgroundAction::ReloadBuffer(action)) => {
                    FileController::apply_async_reload_buffer_result(self, action, results);
                }
                Some(PendingBackgroundAction::ReopenWithEncoding(action)) => {
                    FileController::apply_async_reopen_with_encoding_result(self, action, results);
                }
                Some(PendingBackgroundAction::StartupRestoreCompare(action)) => {
                    self.apply_async_startup_restore_compare_result(action, results);
                }
                Some(PendingBackgroundAction::StartupRestore(_)) | None => {}
            },
            BackgroundIoResult::SessionRestored { request_id, result } => {
                let Some(PendingBackgroundAction::StartupRestore(action)) =
                    self.pending_background_actions.remove(&request_id)
                else {
                    return;
                };
                self.apply_runtime_startup_restore_result(action, result);
            }
        }
    }

    fn apply_runtime_startup_restore_result(
        &mut self,
        action: PendingStartupRestoreAction,
        result: Result<Option<crate::app::services::session_store::RestoredSession>, String>,
    ) {
        let legacy_settings = match result {
            Ok(Some(restored)) => Some(session_manager::apply_restored_session(self, restored)),
            Ok(None) => None,
            Err(error) => {
                self.set_error_status(format!("Session restore failed: {error}"));
                None
            }
        };

        if !action.loaded_from_settings
            && let Some(legacy_settings) = legacy_settings
        {
            self.apply_settings(legacy_settings);
            let _ = self.persist_settings_now();
        }
        self.request_focus_for_active_view();
        self.apply_startup_options_async(action.startup_options);
    }
}

trait BackgroundIoFallback {
    fn into_loaded_path_results(self) -> Option<Vec<LoadedPathResult>>;
    fn into_restore_result(
        self,
    ) -> Result<Option<crate::app::services::session_store::RestoredSession>, String>;
}

impl BackgroundIoFallback for BackgroundIoRequest {
    fn into_loaded_path_results(self) -> Option<Vec<LoadedPathResult>> {
        match self {
            BackgroundIoRequest::LoadPaths { requests, .. } => Some(
                requests
                    .into_iter()
                    .map(|request| LoadedPathResult {
                        path: request.path().clone(),
                        disk_state: None,
                        result: Err("Background file loader unavailable.".to_owned()),
                    })
                    .collect(),
            ),
            BackgroundIoRequest::RestoreSession { .. } => None,
        }
    }

    fn into_restore_result(
        self,
    ) -> Result<Option<crate::app::services::session_store::RestoredSession>, String> {
        match self {
            BackgroundIoRequest::RestoreSession { .. } => {
                Err("Background session restore unavailable.".to_owned())
            }
            BackgroundIoRequest::LoadPaths { .. } => Ok(None),
        }
    }
}
