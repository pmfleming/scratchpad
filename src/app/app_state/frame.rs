use super::{AppSurface, CHROME_TRANSITION_FRAMES, ScratchpadApp};
use crate::app::chrome::handle_window_resize;
use crate::app::diagnostics;
use crate::app::fonts;
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::TabListPosition;
use crate::app::shortcuts;
use crate::app::ui::{callout, dialogs, editor_area, settings, status_bar, tab_strip, transition};
use eframe::egui;
use std::path::PathBuf;

impl ScratchpadApp {
    pub(crate) fn open_encoding_dialog(&mut self) {
        self.state.encoding_dialog_choice = self
            .tab_manager
            .active_tab()
            .map(|tab| tab.active_buffer().format.encoding_name.clone())
            .unwrap_or_else(|| "UTF-8".to_owned());
        self.state.encoding_dialog_open = true;
    }

    pub(crate) fn close_encoding_dialog(&mut self) {
        self.state.encoding_dialog_open = false;
    }

    pub(super) fn handle_pending_close_request(&mut self, ctx: &egui::Context) -> bool {
        if !ctx.input(|input| input.viewport().close_requested()) || self.state.close_in_progress {
            return false;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.request_exit(ctx);
        true
    }

    pub(super) fn prepare_frame(&mut self, ctx: &egui::Context) {
        if self.state.window_shown_after_first_frame {
            self.record_window_state(ctx);
        }
        if handle_window_resize(ctx) && self.state.overflow_popup_open {
            // Rebuild the overflow popup lazily against the resized viewport.
            self.state.overflow_popup_open = false;
        }
        self.tab_manager.evict_inactive_tab_state();
        self.poll_file_watcher(ctx);
        self.poll_background_io(ctx);
        self.handle_dropped_files(ctx);
        self.apply_theme_to_context(ctx);
        crate::app::ui::widget_ids::configure_debug_options(ctx);
        self.sync_editor_fonts(ctx);
        crate::app::services::session_manager::maybe_persist_session(self, ctx);
        callout::set_modal_scroll_blocker_active(ctx, self.modal_callout_open());
        transition::set_chrome_transition_active(ctx, self.chrome_transition_active());
        self.sync_window_title(ctx);
    }

    pub fn prepare_context_before_first_frame(&mut self, ctx: &egui::Context) {
        self.sync_editor_fonts(ctx);
        self.apply_theme_to_context(ctx);
        crate::app::ui::widget_ids::configure_debug_options(ctx);
    }

    pub(super) fn render_frame(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.apply_deferred_layout_settings(ctx);
        paint_root_background(ui, self.state.app_settings.editor_background_color());
        self.render_tab_chrome(ui);
        self.render_active_surface(ui);
        dialogs::show_startup_restore_conflict_modal(ctx, self);
        dialogs::show_pending_action_modal(ctx, self);
        dialogs::show_encoding_window(ctx, self);
        dialogs::show_text_history_window(ctx, self);
        dialogs::show_status_history_window(ctx, self);
        shortcuts::handle_shortcuts(self, ctx);
        self.show_window_after_first_frame(ctx);
        self.finish_frame_transitions(ctx);
    }

    fn render_tab_chrome(&mut self, ui: &mut egui::Ui) {
        if self.state.app_settings.tab_list_position() == TabListPosition::Top {
            tab_strip::show_header(ui, self);
        } else {
            tab_strip::show_top_drag_bar(ui, self);
        }
        if self.state.app_settings.status_bar_visible() {
            status_bar::show_status_bar(ui, self);
        }
        tab_strip::show_bottom_tab_list(ui, self);
        tab_strip::show_vertical_tab_list(ui, self);
    }

    fn render_active_surface(&mut self, ui: &mut egui::Ui) {
        match self.state.active_surface {
            AppSurface::Workspace => editor_area::show_editor(ui, self),
            AppSurface::Settings => settings::show_page(ui, self),
        }
    }

    fn sync_window_title(&mut self, ctx: &egui::Context) {
        let title = self.window_title();
        if self.state.current_window_title.as_ref() == Some(&title) {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        self.state.current_window_title = Some(title);
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let paths = ctx.input(|input| dropped_file_paths(&input.raw.dropped_files));
        if paths.is_empty() {
            return;
        }

        FileController::open_paths_async(self, paths);
    }

    fn show_window_after_first_frame(&mut self, ctx: &egui::Context) {
        if self.state.window_shown_after_first_frame {
            return;
        }
        if self.state.painted_frames_before_window_show < 2 {
            self.state.painted_frames_before_window_show += 1;
            if self.state.painted_frames_before_window_show == 2
                && self.state.app_settings.window_state.maximized
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            }
            ctx.request_repaint();
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        self.state.window_shown_after_first_frame = true;
    }

    fn persist_with_error_status(&mut self) -> bool {
        match self.persist_session_now() {
            Ok(()) => true,
            Err(error) => {
                self.state.status.report_session_save_failed(error);
                false
            }
        }
    }

    pub(crate) fn begin_chrome_transition(&mut self) {
        self.state.chrome_transition_frames_remaining = CHROME_TRANSITION_FRAMES;
    }

    pub(crate) fn begin_layout_transition(&mut self) {
        self.begin_chrome_transition();
    }

    pub(crate) fn chrome_transition_active(&self) -> bool {
        self.state.chrome_transition_frames_remaining > 0
    }

    fn modal_callout_open(&self) -> bool {
        self.state.encoding_dialog_open
            || self.state.text_history_open
            || self.state.status_history_open
            || self.pending_action().is_some()
            || self.current_startup_restore_conflict().is_some()
    }

    fn finish_frame_transitions(&mut self, ctx: &egui::Context) {
        if self.state.chrome_transition_frames_remaining > 0 {
            self.state.chrome_transition_frames_remaining -= 1;
        }
        transition::set_chrome_transition_active(ctx, self.chrome_transition_active());
        if self.chrome_transition_active() {
            ctx.request_repaint();
        }
    }

    fn apply_deferred_layout_settings(&mut self, ctx: &egui::Context) {
        if ctx.current_pass_index() != 0 {
            return;
        }
        if let Some(visible) = self.state.pending_status_bar_visible.take() {
            crate::app::app_state::settings_controller::set_status_bar_visible(self, visible);
        }
    }

    pub(crate) fn estimated_tab_strip_width(&self, spacing: f32) -> f32 {
        let tab_count = self.total_tab_slots();
        if tab_count > 0 {
            (tab_count as f32 * crate::app::theme::TAB_BUTTON_WIDTH)
                + ((tab_count.saturating_sub(1)) as f32 * spacing)
        } else {
            0.0
        }
    }

    pub(crate) fn request_exit(&mut self, ctx: &egui::Context) {
        if self.state.close_in_progress {
            return;
        }

        self.record_window_state(ctx);
        if !self.persist_settings_with_error_status() {
            return;
        }

        if self.persist_with_error_status() {
            self.state.close_in_progress = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn persist_settings_with_error_status(&mut self) -> bool {
        match self.persist_settings_now() {
            Ok(()) => true,
            Err(error) => {
                self.state.status.report_settings_save_failed(error);
                false
            }
        }
    }

    pub(crate) fn window_title(&self) -> String {
        if self.showing_settings() {
            return "Settings - Scratchpad".to_owned();
        }

        if self.tab_manager.tabs.is_empty() {
            return "Scratchpad".to_owned();
        }

        let index = self
            .tab_manager
            .active_tab_index
            .min(self.tab_manager.tabs.len() - 1);
        let tab = &self.tab_manager.tabs[index];
        format!("{} - Scratchpad", tab.active_buffer().name)
    }

    fn sync_editor_fonts(&mut self, ctx: &egui::Context) {
        if self.state.applied_editor_font == Some(self.state.app_settings.editor_font) {
            return;
        }

        if let Err(error) = fonts::apply_editor_fonts(ctx, self.state.app_settings.editor_font) {
            diagnostics::record_warning(
                "apply_editor_font",
                None,
                "app_state::frame",
                format!(
                    "Editor font '{}' unavailable; using default fallback: {error}",
                    self.state.app_settings.editor_font.label()
                ),
            );
            self.state.status.set_warning_status_with_detail(
                crate::app::app_state::StatusDomain::Settings,
                format!(
                    "Could not use editor font '{}'. The default font is in use.",
                    self.state.app_settings.editor_font.label()
                ),
                error.to_string(),
            );
        }
        self.clear_editor_layout_caches();
        self.state.applied_editor_font = Some(self.state.app_settings.editor_font);
    }

    fn clear_editor_layout_caches(&mut self) {
        for tab in self.tab_manager.tabs.as_mut_slice() {
            for view in &mut tab.views {
                view.layout_cache.clear();
            }
        }
    }
}

fn paint_root_background(ui: &egui::Ui, fill: egui::Color32) {
    ui.painter().rect_filled(ui.max_rect(), 0.0, fill);
}

fn dropped_file_paths(dropped_files: &[egui::DroppedFile]) -> Vec<PathBuf> {
    dropped_files
        .iter()
        .filter_map(|file| file.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{dropped_file_paths, egui};
    use std::path::PathBuf;

    #[test]
    fn dropped_file_paths_uses_only_files_with_paths() {
        let first = PathBuf::from(r"C:\notes\one.txt");
        let second = PathBuf::from(r"C:\notes\two.txt");
        let dropped_files = vec![
            egui::DroppedFile {
                path: Some(first.clone()),
                ..Default::default()
            },
            egui::DroppedFile {
                name: "virtual.txt".to_owned(),
                bytes: Some(std::sync::Arc::new([1, 2, 3])),
                ..Default::default()
            },
            egui::DroppedFile {
                path: Some(second.clone()),
                ..Default::default()
            },
        ];

        assert_eq!(dropped_file_paths(&dropped_files), vec![first, second]);
    }
}
