use super::{AppSurface, ScratchpadApp};
use crate::app::domain::TabManager;
use crate::app::logging::{self, LogLevel};
use crate::app::services::file_controller::FileController;
use crate::app::services::session_manager;
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::{AppSettings, SettingsStore};
use crate::app::startup::{StartupOpenTarget, StartupOptions};
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
            settings_store,
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
        };

        let loaded_from_settings = app.load_settings_from_store();
        if startup_options.restore_session {
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
        if startup_options.log_cli {
            logging::log(
                LogLevel::Info,
                &format!("Startup options resolved: {}", startup_options.describe()),
            );
        }

        if startup_options.files.is_empty() {
            if let Some(message) = startup_options.startup_notice {
                self.set_warning_status(message);
            }
            return;
        }

        match startup_options.open_target {
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
}
