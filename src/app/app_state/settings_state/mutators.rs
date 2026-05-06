use super::{
    AppSettings, AppSurface, AppThemeMode, FileController, FileOpenDisposition, NewTabPlacement,
    ScratchpadApp, StartupSessionBehavior, TabListPosition, TabOrderMode, color_to_hex,
    sanitize_tab_list_auto_hide_delay_seconds, stock_editor_palette_for_selection,
};
use crate::app::domain::TextHistoryBudget;
use crate::app::fonts::EditorFontPreset;
use eframe::egui;
use std::time::Instant;

impl ScratchpadApp {
    fn persist_settings_or_error(&mut self) {
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    fn persist_settings_if_changed<T, F>(&mut self, current: T, next: T, apply: F)
    where
        T: PartialEq,
        F: FnOnce(&mut Self, T),
    {
        if current == next {
            return;
        }

        apply(self, next);
        self.persist_settings_or_error();
    }

    fn reset_tab_list_visibility_state(&mut self, keep_open: bool) {
        self.vertical_tab_list_open = keep_open;
        self.vertical_tab_list_hide_deadline = None;
    }

    fn clear_tab_list_hide_deadline(&mut self) {
        self.vertical_tab_list_hide_deadline = None;
    }

    fn set_tab_list_width(&mut self, width: f32) {
        self.app_settings.tab_list_width = width;
        self.persist_settings_or_error();
    }

    fn set_settings_surface(&mut self, surface: AppSurface, open: bool) -> bool {
        let changed = self.settings_tab_open() != open;
        self.settings_tab_index = self.settings_tab_index.min(self.tabs().len());
        self.app_settings.settings_tab_open = open;
        self.active_surface = surface;
        self.ensure_active_tab_slot_selected();
        self.tab_manager.pending_scroll_to_active = true;
        changed
    }

    pub(crate) fn set_font_size(&mut self, font_size: f32) {
        let next = font_size.clamp(8.0, 72.0);
        if (self.app_settings.font_size - next).abs() < f32::EPSILON {
            return;
        }

        self.app_settings.font_size = next;
        self.persist_settings_or_error();
    }

    pub(crate) fn set_editor_font(&mut self, editor_font: EditorFontPreset) {
        self.persist_settings_if_changed(
            self.app_settings.editor_font,
            editor_font,
            |app, next| {
                app.app_settings.editor_font = next;
                app.applied_editor_font = None;
            },
        );
    }

    pub(crate) fn set_word_wrap(&mut self, enabled: bool) {
        self.persist_settings_if_changed(self.app_settings.word_wrap, enabled, |app, next| {
            app.app_settings.word_wrap = next
        });
    }

    pub(crate) fn set_editor_gutter(&mut self, gutter: u8) {
        let next = gutter.min(32);
        self.persist_settings_if_changed(self.app_settings.editor_gutter, next, |app, value| {
            app.app_settings.editor_gutter = value
        });
    }

    pub(crate) fn apply_theme_mode_preset(
        &mut self,
        theme_mode: AppThemeMode,
        system_theme: Option<egui::Theme>,
    ) {
        let (text_color, background_color) =
            stock_editor_palette_for_selection(theme_mode, system_theme);
        if self.app_settings.theme_mode == theme_mode
            && self.app_settings.editor_text_color == text_color
            && self.app_settings.editor_background_color == background_color
        {
            return;
        }

        self.app_settings.theme_mode = theme_mode;
        self.app_settings.editor_text_color = text_color.to_owned();
        self.app_settings.editor_background_color = background_color.to_owned();
        self.persist_settings_or_error();
    }

    pub(crate) fn set_editor_text_color(&mut self, color: egui::Color32) {
        self.set_editor_palette_color(color_to_hex(color), true);
    }

    pub(crate) fn set_editor_background_color(&mut self, color: egui::Color32) {
        self.set_editor_palette_color(color_to_hex(color), false);
    }

    pub(crate) fn set_editor_text_highlight_color(&mut self, color: egui::Color32) {
        let next = color_to_hex(color);
        let next_text = color_to_hex(crate::app::color_contrast::optimal_text_color(color));
        if self.app_settings.editor_text_highlight_color == next
            && self.app_settings.editor_text_highlight_text_color == next_text
        {
            return;
        }

        self.app_settings.editor_text_highlight_color = next;
        self.app_settings.editor_text_highlight_text_color = next_text;
        self.persist_settings_or_error();
    }

    fn set_editor_palette_color(&mut self, next: String, is_text_color: bool) {
        let changed = {
            let current = if is_text_color {
                &mut self.app_settings.editor_text_color
            } else {
                &mut self.app_settings.editor_background_color
            };
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        };

        if changed {
            self.persist_settings_or_error();
        }
    }

    pub(crate) fn set_tab_list_position(&mut self, position: TabListPosition) {
        if self.app_settings.tab_list_position == position {
            return;
        }

        self.app_settings.tab_list_position = position;
        self.begin_layout_transition();
        self.reset_tab_list_visibility_state(false);
        if position.is_vertical() {
            self.overflow_popup_open = false;
        }
        self.tab_manager.pending_scroll_to_active = true;
        self.persist_settings_or_error();
    }

    pub(crate) fn set_tab_order_mode(&mut self, mode: TabOrderMode) {
        if self.app_settings.tab_order_mode == mode {
            return;
        }

        if self.app_settings.tab_order_mode == TabOrderMode::Custom {
            self.remember_current_custom_tab_order();
        }

        self.app_settings.tab_order_mode = mode;
        if mode == TabOrderMode::Custom {
            self.restore_custom_tab_order();
        } else {
            self.apply_current_tab_ordering();
        }
        self.persist_settings_or_error();
    }

    pub(crate) fn set_file_open_disposition(&mut self, disposition: FileOpenDisposition) {
        self.persist_settings_if_changed(
            self.app_settings.file_open_disposition,
            disposition,
            |app, next| app.app_settings.file_open_disposition = next,
        );
    }

    pub(crate) fn set_new_tab_placement(&mut self, placement: NewTabPlacement) {
        self.persist_settings_if_changed(
            self.app_settings.new_tab_placement,
            placement,
            |app, next| app.app_settings.new_tab_placement = next,
        );
    }

    pub(crate) fn set_startup_session_behavior(&mut self, behavior: StartupSessionBehavior) {
        self.persist_settings_if_changed(
            self.app_settings.startup_session_behavior,
            behavior,
            |app, next| app.app_settings.startup_session_behavior = next,
        );
    }

    pub(crate) fn set_auto_hide_tab_list(&mut self, enabled: bool) {
        if self.app_settings.auto_hide_tab_list == enabled {
            return;
        }

        self.app_settings.auto_hide_tab_list = enabled;
        self.begin_layout_transition();
        self.reset_tab_list_visibility_state(enabled && self.vertical_tab_list_open);
        self.persist_settings_or_error();
    }

    pub(crate) fn set_tab_list_auto_hide_delay_seconds(&mut self, seconds: f32) {
        let next = sanitize_tab_list_auto_hide_delay_seconds(seconds);
        if (self.app_settings.tab_list_auto_hide_delay_seconds - next).abs() < f32::EPSILON {
            return;
        }

        self.app_settings.tab_list_auto_hide_delay_seconds = next;
        self.clear_tab_list_hide_deadline();
        self.persist_settings_or_error();
    }

    pub(crate) fn set_recent_files_enabled(&mut self, enabled: bool) {
        self.persist_settings_if_changed(
            self.app_settings.recent_files_enabled,
            enabled,
            |app, next| app.app_settings.recent_files_enabled = next,
        );
    }

    pub(crate) fn set_status_bar_visible(&mut self, visible: bool) {
        if self.app_settings.status_bar_visible == visible {
            self.pending_status_bar_visible = None;
            return;
        }

        self.app_settings.status_bar_visible = visible;
        self.begin_layout_transition();
        self.persist_settings_or_error();
    }

    pub(crate) fn defer_status_bar_visible(&mut self, visible: bool, ctx: &egui::Context) {
        self.pending_status_bar_visible =
            (self.app_settings.status_bar_visible != visible).then_some(visible);
        if self.pending_status_bar_visible.is_some() {
            ctx.request_repaint();
        }
    }

    pub(crate) fn set_history_budget(&mut self, mut budget: TextHistoryBudget) {
        budget = budget.sanitized();
        if self.app_settings.history_budget == budget {
            return;
        }
        budget.derived_from_memory = false;
        self.app_settings.history_budget = budget;
        self.apply_history_budget_to_open_buffers();
        self.persist_settings_or_error();
    }

    pub(crate) fn reset_history_budget_to_auto(&mut self) {
        self.app_settings.history_budget = TextHistoryBudget::derive_from_available_memory();
        self.apply_history_budget_to_open_buffers();
        self.persist_settings_or_error();
    }

    pub(crate) fn set_tab_list_width_from_layout(&mut self, width: f32) {
        let next = width.clamp(
            Self::VERTICAL_TAB_LIST_MIN_WIDTH,
            Self::VERTICAL_TAB_LIST_MAX_WIDTH,
        );
        if (self.app_settings.tab_list_width - next).abs() < 1.0 {
            return;
        }

        self.begin_layout_transition();
        self.set_tab_list_width(next);
    }

    pub(crate) fn open_settings(&mut self) {
        self.reload_settings_before_workspace_change();
        self.begin_layout_transition();
        if !self.settings_tab_open() {
            self.settings_preview_quote_index = (self.settings_preview_quote_index + 1)
                % crate::app::ui::settings::PREVIEW_QUOTES.len();
        }
        if self.set_settings_surface(AppSurface::Settings, true) {
            self.persist_settings_or_error();
        }
    }

    pub(crate) fn open_settings_file_tab(&mut self) {
        let path = self.settings_path().to_path_buf();
        self.activate_workspace_surface();
        FileController::open_paths_async(self, vec![path]);
    }

    pub(crate) fn close_settings(&mut self) {
        self.begin_layout_transition();
        if self.set_settings_surface(AppSurface::Workspace, false) {
            self.persist_settings_or_error();
        }
        self.request_focus_for_active_view();
    }

    pub(crate) fn reset_settings_to_defaults(&mut self) {
        let defaults = AppSettings {
            settings_tab_open: self.settings_tab_open(),
            settings_tab_index: Some(self.settings_tab_index.min(self.tabs().len())),
            ..AppSettings::default()
        };
        self.apply_settings(defaults);
        self.applied_editor_font = None;
        match self.persist_settings_now() {
            Ok(()) => self.set_info_status("Settings reset to defaults."),
            Err(error) => self.set_error_status(format!("Settings save failed: {error}")),
        }
    }

    pub(crate) fn apply_workspace_tab_order(&mut self, workspace_order: Vec<usize>) {
        if self.apply_workspace_tab_order_internal(workspace_order, false) {
            self.app_settings.tab_order_mode = TabOrderMode::Custom;
            self.remember_current_custom_tab_order();
            self.persist_settings_or_error();
        }
    }

    pub(crate) fn apply_current_tab_ordering(&mut self) -> bool {
        let workspace_order = match workspace_tab_order_for_mode(self, self.tab_order_mode()) {
            Some(order) => order,
            None => return false,
        };
        let reordered = self.apply_workspace_tab_order_internal(workspace_order, false);
        if reordered {
            self.begin_layout_transition();
        }
        reordered
    }

    fn apply_workspace_tab_order_internal(
        &mut self,
        workspace_order: Vec<usize>,
        persist_settings: bool,
    ) -> bool {
        if workspace_order.len() != self.tab_manager.tabs.len() {
            return false;
        }

        let active_workspace_index = self.tab_manager.active_tab_index;
        let current_order = (0..self.tab_manager.tabs.len()).collect::<Vec<_>>();
        if workspace_order == current_order {
            return false;
        }

        let mut tabs = std::mem::take(&mut self.tab_manager.tabs)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        self.tab_manager.tabs = workspace_order
            .iter()
            .filter_map(|&index| tabs.get_mut(index).and_then(Option::take))
            .collect();
        self.tab_manager.active_tab_index = workspace_order
            .iter()
            .position(|&index| index == active_workspace_index)
            .unwrap_or(0);
        self.ensure_active_tab_slot_selected();
        self.tab_manager.pending_scroll_to_active = true;
        self.mark_session_dirty();
        if persist_settings {
            self.persist_settings_or_error();
        }
        true
    }

    pub(crate) fn remember_current_custom_tab_order(&mut self) {
        self.app_settings.custom_tab_order = self
            .tabs()
            .iter()
            .map(|tab| tab.buffer.id)
            .collect::<Vec<_>>();
    }

    fn restore_custom_tab_order(&mut self) -> bool {
        let workspace_order = workspace_tab_order_from_saved_custom_order(self);
        let reordered = self.apply_workspace_tab_order_internal(workspace_order, false);
        if reordered {
            self.begin_layout_transition();
        }
        reordered
    }

    pub(crate) fn activate_workspace_surface(&mut self) {
        self.active_surface = AppSurface::Workspace;
    }

    pub(crate) fn keep_tab_list_open(&mut self) {
        self.reset_tab_list_visibility_state(true);
    }

    pub(crate) fn delay_tab_list_hide(&mut self, now: Instant) {
        self.vertical_tab_list_open = true;
        self.vertical_tab_list_hide_deadline = Some(now + self.tab_list_auto_hide_delay());
    }

    pub(crate) fn close_tab_list(&mut self) {
        self.reset_tab_list_visibility_state(false);
    }
}

fn workspace_tab_order_for_mode(app: &ScratchpadApp, mode: TabOrderMode) -> Option<Vec<usize>> {
    if mode == TabOrderMode::Custom {
        return None;
    }

    let mut order = (0..app.tabs().len()).collect::<Vec<_>>();
    let custom_rank = custom_order_ranks(app);
    match mode {
        TabOrderMode::Custom => None,
        TabOrderMode::FileName => {
            order.sort_by(|left, right| {
                let left_tab = &app.tabs()[*left];
                let right_tab = &app.tabs()[*right];
                left_tab
                    .buffer
                    .name
                    .to_ascii_lowercase()
                    .cmp(&right_tab.buffer.name.to_ascii_lowercase())
                    .then_with(|| left_tab.buffer.name.cmp(&right_tab.buffer.name))
                    .then_with(|| tab_path_label(left_tab).cmp(&tab_path_label(right_tab)))
                    .then_with(|| custom_rank[*left].cmp(&custom_rank[*right]))
            });
            Some(order)
        }
        TabOrderMode::FileAge => {
            order.sort_by_key(|index| {
                let millis = app.tabs()[*index]
                    .buffer
                    .disk_state
                    .as_ref()
                    .and_then(|state| state.modified_millis);
                (
                    millis.is_none(),
                    millis.unwrap_or(u64::MAX),
                    custom_rank[*index],
                )
            });
            Some(order)
        }
        TabOrderMode::RecentEdit => {
            order.sort_by_key(|index| {
                let latest = app.tabs()[*index]
                    .buffers()
                    .flat_map(|buffer| buffer.document().history_entries())
                    .map(|entry| entry.global_seq)
                    .max();
                (
                    latest.is_none(),
                    std::cmp::Reverse(latest.unwrap_or(0)),
                    custom_rank[*index],
                )
            });
            Some(order)
        }
    }
}

fn workspace_tab_order_from_saved_custom_order(app: &ScratchpadApp) -> Vec<usize> {
    let mut order = Vec::with_capacity(app.tabs().len());
    for buffer_id in &app.app_settings.custom_tab_order {
        if let Some(index) = app
            .tabs()
            .iter()
            .position(|tab| tab.buffer.id == *buffer_id)
            && !order.contains(&index)
        {
            order.push(index);
        }
    }
    for index in 0..app.tabs().len() {
        if !order.contains(&index) {
            order.push(index);
        }
    }
    order
}

fn custom_order_ranks(app: &ScratchpadApp) -> Vec<usize> {
    let saved_order = workspace_tab_order_from_saved_custom_order(app);
    let mut ranks = vec![usize::MAX; app.tabs().len()];
    for (rank, index) in saved_order.into_iter().enumerate() {
        ranks[index] = rank;
    }
    ranks
}

fn tab_path_label(tab: &crate::app::domain::WorkspaceTab) -> String {
    tab.buffer
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::domain::{BufferState, DiskFileState, PieceSource, TabManager, WorkspaceTab};
    use crate::app::services::session_store::SessionStore;
    use crate::app::services::settings_store::SettingsStore;
    use crate::app::startup::StartupOptions;
    use crate::app::ui::editor_content::native_editor::{
        CharCursor, CursorRange, EditOperation, OperationRecord,
    };

    #[test]
    fn file_name_order_sorts_workspace_tabs_and_preserves_active_tab() {
        let mut app = test_app(["zeta.txt", "Alpha.txt", "beta.txt"]);
        app.tab_manager.active_tab_index = 0;

        app.set_tab_order_mode(TabOrderMode::FileName);

        assert_eq!(tab_names(&app), ["Alpha.txt", "beta.txt", "zeta.txt"]);
        assert_eq!(app.active_tab().unwrap().buffer.name, "zeta.txt");
    }

    #[test]
    fn file_age_order_uses_disk_modified_time_and_places_unsaved_tabs_last() {
        let mut app = test_app(["newer.txt", "untitled", "older.txt"]);
        app.tab_manager.tabs[0].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(300),
            len: 0,
        });
        app.tab_manager.tabs[2].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(100),
            len: 0,
        });

        app.set_tab_order_mode(TabOrderMode::FileAge);

        assert_eq!(tab_names(&app), ["older.txt", "newer.txt", "untitled"]);
    }

    #[test]
    fn recent_edit_order_uses_latest_text_history_sequence() {
        let mut app = test_app(["alpha.txt", "beta.txt", "gamma.txt"]);
        record_edit(&mut app.tab_manager.tabs[1].buffer, "b");
        record_edit(&mut app.tab_manager.tabs[2].buffer, "g");

        app.set_tab_order_mode(TabOrderMode::RecentEdit);

        assert_eq!(tab_names(&app), ["gamma.txt", "beta.txt", "alpha.txt"]);
    }

    #[test]
    fn manual_display_reorder_switches_back_to_custom_order() {
        let mut app = test_app(["alpha.txt", "beta.txt", "gamma.txt"]);
        app.set_tab_order_mode(TabOrderMode::FileName);

        assert!(app.reorder_display_tab(0, 2));

        assert_eq!(app.tab_order_mode(), TabOrderMode::Custom);
        assert_eq!(tab_names(&app), ["beta.txt", "gamma.txt", "alpha.txt"]);
    }

    #[test]
    fn custom_order_restores_order_from_before_automatic_sort() {
        let mut app = test_app(["zeta.txt", "Alpha.txt", "beta.txt"]);

        app.set_tab_order_mode(TabOrderMode::FileName);
        assert_eq!(tab_names(&app), ["Alpha.txt", "beta.txt", "zeta.txt"]);

        app.set_tab_order_mode(TabOrderMode::Custom);

        assert_eq!(tab_names(&app), ["zeta.txt", "Alpha.txt", "beta.txt"]);
    }

    #[test]
    fn custom_order_survives_switching_between_automatic_modes() {
        let mut app = test_app(["zeta.txt", "Alpha.txt", "beta.txt"]);
        app.tab_manager.tabs[0].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(300),
            len: 0,
        });
        app.tab_manager.tabs[1].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(100),
            len: 0,
        });
        app.tab_manager.tabs[2].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(200),
            len: 0,
        });

        app.set_tab_order_mode(TabOrderMode::FileName);
        app.set_tab_order_mode(TabOrderMode::FileAge);
        assert_eq!(tab_names(&app), ["Alpha.txt", "beta.txt", "zeta.txt"]);

        app.set_tab_order_mode(TabOrderMode::Custom);

        assert_eq!(tab_names(&app), ["zeta.txt", "Alpha.txt", "beta.txt"]);
    }

    fn test_app<const N: usize>(names: [&str; N]) -> ScratchpadApp {
        let temp_dir = tempfile::tempdir().expect("create temp app root");
        let root = temp_dir.keep();
        let mut app = ScratchpadApp::with_stores_and_startup(
            SessionStore::new(root.clone()),
            SettingsStore::new(root),
            StartupOptions::default(),
        );
        app.set_session_persist_on_drop(false);
        app.tab_manager = TabManager {
            tabs: names.into_iter().map(test_tab).collect(),
            active_tab_index: 0,
            pending_action: None,
            session_dirty: false,
            pending_scroll_to_active: false,
        };
        app.clear_tab_selection();
        app
    }

    fn test_tab(name: &str) -> WorkspaceTab {
        WorkspaceTab::new(BufferState::new(name.to_owned(), String::new(), None))
    }

    fn tab_names(app: &ScratchpadApp) -> Vec<String> {
        app.tabs()
            .iter()
            .map(|tab| tab.buffer.name.clone())
            .collect()
    }

    fn record_edit(buffer: &mut BufferState, text: &str) {
        buffer.push_text_edit_operation_with_source(
            OperationRecord {
                previous_cursor: CursorRange::one(CharCursor::new(0)),
                next_cursor: CursorRange::one(CharCursor::new(text.chars().count())),
                edits: vec![EditOperation {
                    start_char: 0,
                    deleted_text: String::new(),
                    inserted_text: text.to_owned(),
                    deleted_spans: Vec::new(),
                }],
            },
            PieceSource::Edit,
        );
    }
}
