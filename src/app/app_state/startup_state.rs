use super::{
    AppPersistenceState, BackgroundIoState, ChromeState, DialogState, FileWatchState, FocusState,
    ScratchpadApp, ScratchpadAppState, SearchState, StatusDomain, WindowRuntimeState,
};
use crate::app::app_state::settings_state;
use crate::app::app_state::workspace::accessors as workspace_accessors;
use crate::app::diagnostics;
use crate::app::domain::TabManager;
use crate::app::services::background_io::spawn_background_io_worker;
use crate::app::services::file_controller::FileController;
use crate::app::services::manual_files;
use crate::app::services::session_manager;
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::{
    AppSettings, FileOpenDisposition, SettingsStore, StartupSessionBehavior,
};
use crate::app::startup::{StartupOpenTarget, StartupOptions};
use std::collections::{HashSet, VecDeque};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupExecution {
    Blocking,
    Background,
}

#[cfg(test)]
mod tests;

impl ScratchpadApp {
    pub(crate) fn set_session_persist_on_drop(&mut self, enabled: bool) {
        self.state.persistence.persist_session_on_drop = enabled;
    }

    #[must_use]
    pub fn with_session_store(session_store: SessionStore) -> Self {
        let settings_root = session_store.root().to_path_buf();
        Self::with_stores_and_startup(
            session_store,
            SettingsStore::new(settings_root),
            StartupOptions::default(),
        )
    }

    #[must_use]
    pub fn with_startup_options(startup_options: StartupOptions) -> Self {
        Self::with_session_store_and_startup(SessionStore::default(), startup_options)
    }

    #[must_use]
    pub fn with_runtime_startup_options(startup_options: StartupOptions) -> Self {
        let session_store = SessionStore::default();
        let settings_root = session_store.root().to_path_buf();
        Self::with_stores_and_runtime_startup(
            session_store,
            SettingsStore::new(settings_root),
            startup_options,
        )
    }

    #[must_use]
    pub fn with_session_store_and_startup(
        session_store: SessionStore,
        startup_options: StartupOptions,
    ) -> Self {
        let settings_root = session_store.root().to_path_buf();
        Self::with_stores_and_startup(
            session_store,
            SettingsStore::new(settings_root),
            startup_options,
        )
    }

    #[must_use]
    pub fn with_stores_and_startup(
        session_store: SessionStore,
        settings_store: SettingsStore,
        startup_options: StartupOptions,
    ) -> Self {
        Self::build_app(
            session_store,
            settings_store,
            startup_options,
            StartupExecution::Blocking,
        )
    }

    #[must_use]
    pub fn with_stores_and_runtime_startup(
        session_store: SessionStore,
        settings_store: SettingsStore,
        startup_options: StartupOptions,
    ) -> Self {
        Self::build_app(
            session_store,
            settings_store,
            startup_options,
            StartupExecution::Background,
        )
    }

    fn build_app(
        session_store: SessionStore,
        settings_store: SettingsStore,
        startup_options: StartupOptions,
        execution: StartupExecution,
    ) -> Self {
        diagnostics::initialize(diagnostics::error_log_path(session_store.root()));

        let (background_io_tx, background_io_rx) = spawn_background_io_worker();
        let mut app = Self {
            tab_manager: TabManager::default(),
            state: ScratchpadAppState {
                app_settings: AppSettings::default(),
                status: super::StatusState::default(),
                focus: FocusState::default(),
                dialogs: DialogState::new(),
                persistence: AppPersistenceState {
                    settings_store,
                    session_store,
                    persist_session_on_drop: true,
                    session_restore_failed: false,
                    pending_closed_tabs: Vec::new(),
                    last_session_persist: Instant::now(),
                },
                user_manual_path: manual_files::resolve_user_manual_path(),
                window: WindowRuntimeState::default(),
                chrome: ChromeState::default(),
                settings_tab_index: usize::MAX,
                pending_settings_toml_refresh: None,
                text_history_cache: crate::app::text_history::TextHistoryCache::default(),
                search_state: SearchState::default(),
                workspace_selection:
                    crate::app::app_state::workspace::display_tabs::WorkspaceSelectionState::default(
                    ),
                pending_open_file_paths: HashSet::new(),
                pending_open_file_dialog: None,
                recently_closed_files: VecDeque::default(),
                workspace_reflow_axis: crate::app::domain::SplitAxis::Vertical,
                settings_preview_quote_index: 2,
                background_io: BackgroundIoState::new(background_io_tx, background_io_rx),
                file_watch: FileWatchState::default(),
                broker_inbox: None,
                deferred_launch_requests: VecDeque::new(),
            },
        };

        let loaded_from_settings = settings_state::load_settings_from_store(&mut app);
        let restore = app.should_restore_session(&startup_options);
        if restore && execution == StartupExecution::Background {
            app.queue_background_session_restore(startup_options, loaded_from_settings);
        } else {
            let legacy = restore
                .then(|| session_manager::restore_session_state(&mut app))
                .flatten();
            let restored = legacy.is_some();
            app.adopt_legacy_startup_settings(loaded_from_settings, legacy);
            app.finish_startup(startup_options, restored, execution);
        }

        crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(&mut app);
        app.tab_manager.pending_scroll_to_active = false;

        app
    }

    pub(super) fn adopt_legacy_startup_settings(
        &mut self,
        loaded_from_settings: bool,
        legacy: Option<AppSettings>,
    ) {
        if !loaded_from_settings && let Some(settings) = legacy {
            settings_state::apply_settings(self, settings);
            let _ = settings_state::persist_settings_now(self);
        }
    }

    pub(super) fn finish_startup(
        &mut self,
        options: StartupOptions,
        restored: bool,
        execution: StartupExecution,
    ) {
        if !restored && options.files.is_empty() {
            self.initialize_default_workspace_tabs();
        }
        workspace_accessors::request_focus_for_active_view(self);
        self.apply_startup_options(options, execution);
    }

    pub(crate) fn apply_startup_options_async(&mut self, options: StartupOptions) {
        self.apply_startup_options(options, StartupExecution::Background);
    }

    fn apply_startup_options(&mut self, options: StartupOptions, execution: StartupExecution) {
        if !options.files.is_empty() {
            use StartupExecution::{Background, Blocking};
            match (self.resolved_startup_open_target(&options), execution) {
                (StartupOpenTarget::SeparateTabs, Blocking) => {
                    FileController::open_external_paths(self, options.files);
                }
                (StartupOpenTarget::SeparateTabs, Background) => {
                    FileController::open_external_paths_async(self, options.files);
                }
                (StartupOpenTarget::ActiveTab, Blocking) => {
                    FileController::open_external_paths_here(self, options.files);
                }
                (StartupOpenTarget::ActiveTab, Background) => {
                    FileController::open_external_paths_here_async(self, options.files);
                }
                (StartupOpenTarget::TabIndex(index), Blocking) => {
                    FileController::open_external_paths_into_tab(self, index, options.files);
                }
                (StartupOpenTarget::TabIndex(index), Background) => {
                    FileController::open_external_paths_into_tab_async(self, index, options.files);
                }
            }
        }
        if let Some(message) = options.startup_notice {
            self.state
                .status
                .set_warning_status_in_domain(StatusDomain::Session, message);
        }
    }

    fn should_restore_session(&self, startup_options: &StartupOptions) -> bool {
        if startup_options.restore_session_explicit {
            startup_options.restore_session
        } else {
            matches!(
                self.state.app_settings.workspace.startup_session_behavior,
                StartupSessionBehavior::ContinuePreviousSession
            )
        }
    }

    fn resolved_startup_open_target(&self, startup_options: &StartupOptions) -> StartupOpenTarget {
        if startup_options.open_target_explicit {
            startup_options.open_target
        } else {
            match self.state.app_settings.workspace.file_open_disposition {
                FileOpenDisposition::NewTab => StartupOpenTarget::SeparateTabs,
                FileOpenDisposition::CurrentTab => StartupOpenTarget::ActiveTab,
            }
        }
    }

    pub(super) fn initialize_default_workspace_tabs(&mut self) {
        self.tab_manager
            .set_tabs(vec![crate::app::domain::WorkspaceTab::untitled()], 0);
        crate::app::app_state::search_runtime::mark_search_dirty(self);
    }
}
