use super::{AppSurface, ScratchpadApp, SearchState};
use crate::app::domain::TabManager;
use crate::app::services::file_controller::FileController;
use crate::app::services::manual_files;
use crate::app::services::session_manager;
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::{
    AppSettings, FileOpenDisposition, SettingsStore, StartupSessionBehavior,
};
use crate::app::startup::{StartupOpenTarget, StartupOptions};
use crate::app::transactions::TransactionLog;
use std::collections::BTreeSet;
use std::time::Instant;

impl ScratchpadApp {
    pub fn with_session_store(session_store: SessionStore) -> Self {
        let settings_root = session_store.root().to_path_buf();
        Self::with_stores_and_startup(
            session_store,
            SettingsStore::new(settings_root),
            StartupOptions::default(),
        )
    }

    pub fn with_startup_options(startup_options: StartupOptions) -> Self {
        let session_store = SessionStore::default();
        let settings_root = session_store.root().to_path_buf();
        Self::with_stores_and_startup(
            session_store,
            SettingsStore::new(settings_root),
            startup_options,
        )
    }

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

    pub fn with_stores_and_startup(
        session_store: SessionStore,
        settings_store: SettingsStore,
        startup_options: StartupOptions,
    ) -> Self {
        let mut app = Self {
            tab_manager: TabManager::default(),
            app_settings: AppSettings::default(),
            status_message: None,
            pending_editor_focus: None,
            encoding_dialog_open: false,
            encoding_dialog_choice: "UTF-8".to_owned(),
            settings_store,
            user_manual_path: manual_files::resolve_user_manual_path(),
            session_store,
            last_session_persist: Instant::now(),
            close_in_progress: false,
            overflow_popup_open: false,
            applied_editor_font: None,
            active_surface: AppSurface::Workspace,
            settings_tab_index: usize::MAX,
            pending_settings_toml_refresh: None,
            vertical_tab_list_open: false,
            vertical_tab_list_hide_deadline: None,
            transaction_log: TransactionLog::default(),
            transaction_log_open: false,
            pending_layout_transaction: None,
            pending_text_transaction: None,
            search_state: SearchState::default(),
            chrome_transition_frames_remaining: 0,
            selected_tab_slots: BTreeSet::new(),
            tab_selection_anchor: None,
            tab_rename_state: None,
            workspace_reflow_axis: crate::app::domain::SplitAxis::Vertical,
            settings_preview_quote_index: 2,
        };

        let loaded_from_settings = app.load_settings_from_store();
        if app.should_restore_session(&startup_options) {
            let legacy_settings = session_manager::restore_session_state(&mut app);
            if !loaded_from_settings && let Some(legacy_settings) = legacy_settings {
                app.apply_settings(legacy_settings);
                let _ = app.persist_settings_now();
            }
        }
        app.request_focus_for_active_view();
        app.apply_startup_options(startup_options);

        app
    }

    fn apply_startup_options(&mut self, startup_options: StartupOptions) {
        if startup_options.files.is_empty() {
            if let Some(message) = startup_options.startup_notice {
                self.set_warning_status(message);
            }
            return;
        }

        let open_target = self.resolved_startup_open_target(&startup_options);

        match open_target {
            StartupOpenTarget::SeparateTabs => {
                FileController::open_external_paths(self, startup_options.files)
            }
            StartupOpenTarget::ActiveTab => {
                FileController::open_external_paths_here(self, startup_options.files)
            }
            StartupOpenTarget::TabIndex(index) => {
                FileController::open_external_paths_into_tab(self, index, startup_options.files)
            }
        }

        if let Some(message) = startup_options.startup_notice {
            self.set_warning_status(message);
        }
    }

    fn should_restore_session(&self, startup_options: &StartupOptions) -> bool {
        if startup_options.restore_session_explicit {
            startup_options.restore_session
        } else {
            matches!(
                self.app_settings.startup_session_behavior,
                StartupSessionBehavior::ContinuePreviousSession
            )
        }
    }

    fn resolved_startup_open_target(&self, startup_options: &StartupOptions) -> StartupOpenTarget {
        if startup_options.open_target_explicit {
            startup_options.open_target
        } else {
            match self.app_settings.file_open_disposition {
                FileOpenDisposition::NewTab => StartupOpenTarget::SeparateTabs,
                FileOpenDisposition::CurrentTab => StartupOpenTarget::ActiveTab,
            }
        }
    }
}
