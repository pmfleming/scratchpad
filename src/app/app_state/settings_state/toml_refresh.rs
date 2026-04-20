use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{BufferId, BufferState, ViewId};
use crate::app::services::settings_store::parse_toml_settings;

pub(in crate::app::app_state) enum SettingsTomlRefreshAction {
    ApplyBuffer(String),
}

pub(in crate::app::app_state) type SettingsTomlRefresh =
    Option<(BufferId, SettingsTomlRefreshAction)>;

impl ScratchpadApp {
    pub(crate) fn reload_settings_from_active_settings_tab(&mut self) {
        let refresh = self.active_settings_toml_refresh_action();
        self.apply_settings_toml_refresh(refresh);
    }

    pub(crate) fn reload_settings_before_workspace_change(&mut self) {
        self.reload_settings_from_active_settings_tab();
    }

    pub(crate) fn reload_settings_if_switching_views(&mut self, next_view_id: ViewId) {
        if self
            .active_tab()
            .is_some_and(|tab| tab.active_view_id != next_view_id)
        {
            self.reload_settings_from_active_settings_tab();
        }
    }

    pub(crate) fn reload_settings_if_closing_view(&mut self, view_id: ViewId) {
        if self
            .active_tab()
            .is_some_and(|tab| tab.active_view_id == view_id)
        {
            self.reload_settings_from_active_settings_tab();
        }
    }

    pub(crate) fn note_settings_toml_edit(&mut self, tab_index: usize) {
        let Some((buffer_id, _, _)) = self.active_settings_file_buffer_snapshot(tab_index) else {
            return;
        };

        self.pending_settings_toml_refresh = Some(buffer_id);
    }

    pub(in crate::app::app_state) fn settings_toml_refresh_on_tab_close(
        &self,
        index: usize,
    ) -> SettingsTomlRefresh {
        let (buffer_id, raw, is_dirty) = self.settings_file_buffer_snapshot(index)?;
        let action = if is_dirty || self.pending_settings_toml_refresh == Some(buffer_id) {
            SettingsTomlRefreshAction::ApplyBuffer(raw)
        } else {
            return None;
        };

        Some((buffer_id, action))
    }

    pub(in crate::app::app_state) fn apply_settings_toml_refresh(
        &mut self,
        refresh: SettingsTomlRefresh,
    ) {
        let Some((buffer_id, action)) = refresh else {
            return;
        };

        self.clear_pending_settings_toml_refresh(buffer_id);
        self.apply_settings_toml_refresh_action(action);
    }

    fn apply_settings_toml_refresh_action(&mut self, action: SettingsTomlRefreshAction) {
        match action {
            SettingsTomlRefreshAction::ApplyBuffer(raw) => match parse_toml_settings(&raw) {
                Ok(settings) => {
                    self.apply_settings(settings);
                    self.applied_editor_font = None;
                    self.set_info_status("Settings reloaded from settings.toml.");
                }
                Err(error) => {
                    self.set_warning_status(format!("Settings TOML parse failed: {error}"));
                }
            },
        }
    }

    fn clear_pending_settings_toml_refresh(&mut self, buffer_id: BufferId) {
        if self.pending_settings_toml_refresh == Some(buffer_id) {
            self.pending_settings_toml_refresh = None;
        }
    }

    fn active_settings_toml_refresh_action(&self) -> SettingsTomlRefresh {
        let (buffer_id, raw, is_dirty) =
            self.active_settings_file_buffer_snapshot(self.active_tab_index())?;
        if !is_dirty && self.pending_settings_toml_refresh != Some(buffer_id) {
            return None;
        }

        Some((buffer_id, SettingsTomlRefreshAction::ApplyBuffer(raw)))
    }

    fn active_settings_file_buffer_snapshot(
        &self,
        index: usize,
    ) -> Option<(BufferId, String, bool)> {
        let buffer = self.tabs().get(index)?.active_buffer();
        buffer
            .is_settings_file
            .then(|| settings_buffer_snapshot(buffer))
    }

    fn settings_file_buffer_snapshot(&self, index: usize) -> Option<(BufferId, String, bool)> {
        let tab = self.tabs().get(index)?;
        let buffer = tab.buffers().find(|buffer| buffer.is_settings_file)?;

        Some(settings_buffer_snapshot(buffer))
    }
}

fn settings_buffer_snapshot(buffer: &BufferState) -> (BufferId, String, bool) {
    (buffer.id, buffer.text(), buffer.is_dirty)
}
