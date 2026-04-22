use super::{AppSurface, CHROME_TRANSITION_FRAMES, ScratchpadApp};
use crate::app::chrome::handle_window_resize;
use crate::app::fonts;
use crate::app::services::settings_store::TabListPosition;
use crate::app::shortcuts;
use crate::app::ui::{dialogs, editor_area, settings, status_bar, tab_strip, transition};
use eframe::egui;

impl ScratchpadApp {
    pub(crate) fn open_encoding_dialog(&mut self) {
        self.encoding_dialog_choice = self
            .active_tab()
            .map(|tab| tab.active_buffer().format.encoding_name.clone())
            .unwrap_or_else(|| "UTF-8".to_owned());
        self.encoding_dialog_open = true;
    }

    pub(crate) fn close_encoding_dialog(&mut self) {
        self.encoding_dialog_open = false;
    }

    pub(super) fn handle_pending_close_request(&mut self, ctx: &egui::Context) -> bool {
        if !ctx.input(|input| input.viewport().close_requested()) || self.close_in_progress {
            return false;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.request_exit(ctx);
        true
    }

    pub(super) fn prepare_frame(&mut self, ctx: &egui::Context) {
        if handle_window_resize(ctx) && self.overflow_popup_open {
            // Rebuild the overflow popup lazily against the resized viewport.
            self.overflow_popup_open = false;
        }
        self.poll_background_io(ctx);
        self.apply_theme_to_context(ctx);
        crate::app::ui::widget_ids::configure_debug_options(ctx);
        self.sync_editor_fonts(ctx);
        crate::app::services::session_manager::maybe_persist_session(self, ctx);
        transition::set_chrome_transition_active(ctx, self.chrome_transition_active());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
    }

    pub(super) fn render_frame(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        paint_root_background(ui, self.editor_background_color());
        self.render_tab_chrome(ui);
        self.render_active_surface(ui);
        dialogs::show_startup_restore_conflict_modal(ctx, self);
        dialogs::show_pending_action_modal(ctx, self);
        dialogs::show_encoding_window(ctx, self);
        dialogs::show_transaction_log_window(ctx, self);
        shortcuts::handle_shortcuts(self, ctx);
        self.finish_frame_transitions(ctx);
    }

    fn render_tab_chrome(&mut self, ui: &mut egui::Ui) {
        if self.tab_list_position() == TabListPosition::Top {
            tab_strip::show_header(ui, self);
        } else {
            tab_strip::show_top_drag_bar(ui, self);
        }
        if self.status_bar_visible() {
            status_bar::show_status_bar(ui, self);
        }
        tab_strip::show_bottom_tab_list(ui, self);
        tab_strip::show_vertical_tab_list(ui, self);
    }

    fn render_active_surface(&mut self, ui: &mut egui::Ui) {
        match self.active_surface {
            AppSurface::Workspace => editor_area::show_editor(ui, self),
            AppSurface::Settings => settings::show_page(ui, self),
        }
    }

    fn persist_with_error_status(&mut self, error_prefix: &str) -> bool {
        match self.persist_session_now() {
            Ok(()) => true,
            Err(error) => {
                self.set_error_status(format!("{error_prefix}: {error}"));
                false
            }
        }
    }

    pub(crate) fn begin_chrome_transition(&mut self) {
        self.chrome_transition_frames_remaining = CHROME_TRANSITION_FRAMES;
    }

    pub(crate) fn begin_layout_transition(&mut self) {
        self.begin_chrome_transition();
    }

    pub(crate) fn chrome_transition_active(&self) -> bool {
        self.chrome_transition_frames_remaining > 0
    }

    fn finish_frame_transitions(&mut self, ctx: &egui::Context) {
        if self.chrome_transition_frames_remaining > 0 {
            self.chrome_transition_frames_remaining -= 1;
        }
        transition::set_chrome_transition_active(ctx, self.chrome_transition_active());
        if self.chrome_transition_active() {
            ctx.request_repaint();
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
        if self.close_in_progress {
            return;
        }

        if self.persist_with_error_status("Session save failed") {
            self.close_in_progress = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
        let marker = if tab.active_buffer().is_dirty {
            "*"
        } else {
            ""
        };
        format!("{}{} - Scratchpad", marker, tab.active_buffer().name)
    }

    fn sync_editor_fonts(&mut self, ctx: &egui::Context) {
        if self.applied_editor_font == Some(self.app_settings.editor_font) {
            return;
        }

        if let Err(error) = fonts::apply_editor_fonts(ctx, self.app_settings.editor_font) {
            self.set_warning_status(format!(
                "Editor font '{}' unavailable; using default fallback: {error}",
                self.app_settings.editor_font.label()
            ));
        }
        self.applied_editor_font = Some(self.app_settings.editor_font);
    }
}

fn paint_root_background(ui: &egui::Ui, fill: egui::Color32) {
    ui.painter().rect_filled(ui.max_rect(), 0.0, fill);
}
