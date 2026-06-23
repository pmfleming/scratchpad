use super::{AppSurface, ScratchpadApp};
use crate::app::app_state::settings_state;
use crate::app::app_state::workspace::restore_conflict;
use crate::app::capacity_metrics::{FramePhase, record_frame_phase};
use crate::app::chrome::{handle_window_resize, show_window_resize_cursor};
use crate::app::diagnostics;
use crate::app::fonts;
use crate::app::platform;
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::TabListPosition;
use crate::app::shortcuts;
use crate::app::ui::{callout, dialogs, editor_area, settings, status_bar, tab_strip, transition};
use eframe::egui;
use std::path::PathBuf;
use std::time::Instant;

pub(crate) fn open_encoding_dialog(app: &mut ScratchpadApp) {
    let choice = app.tab_manager.active_tab().map_or_else(
        || "UTF-8".to_owned(),
        |tab| tab.active_buffer().format.encoding_name.clone(),
    );
    app.state.dialogs.encoding.open_with_choice(choice);
}

pub(crate) fn close_encoding_dialog(app: &mut ScratchpadApp) {
    app.state.dialogs.encoding.close();
}

pub(super) fn handle_pending_close_request(app: &mut ScratchpadApp, ctx: &egui::Context) -> bool {
    if !ctx.input(|input| input.viewport().close_requested()) || app.state.close_in_progress {
        return false;
    }

    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
    request_exit(app, ctx);
    true
}

pub(super) fn prepare_frame(app: &mut ScratchpadApp, ctx: &egui::Context) {
    let started_at = Instant::now();
    if app.state.window_shown_after_first_frame {
        app.record_window_state(ctx);
    }
    if handle_window_resize(ctx, platform_capabilities(app).allow_app_resize_grips)
        && app.state.overflow_popup_open
    {
        // Rebuild the overflow popup lazily against the resized viewport.
        app.state.overflow_popup_open = false;
    }
    app.tab_manager.evict_inactive_tab_state();
    app.poll_file_watcher(ctx);
    FileController::poll_open_file_dialog(app, ctx);
    let background_poll_started_at = Instant::now();
    app.poll_background_io(ctx);
    record_frame_phase(
        FramePhase::BackgroundPoll,
        background_poll_started_at.elapsed(),
    );
    handle_dropped_files(app, ctx);
    settings_state::apply_theme_to_context(app, ctx);
    crate::app::ui::widget_ids::configure_debug_options(ctx);
    sync_editor_fonts(app, ctx);
    crate::app::services::session_manager::maybe_persist_session(app, ctx);
    callout::set_modal_scroll_blocker_active(ctx, modal_callout_open(app));
    transition::set_chrome_transition_active(ctx, chrome_transition_active(app));
    sync_window_title(app, ctx);
    record_frame_phase(FramePhase::Prepare, started_at.elapsed());
}

pub fn prepare_context_before_first_frame(app: &mut ScratchpadApp, ctx: &egui::Context) {
    sync_editor_fonts(app, ctx);
    settings_state::apply_theme_to_context(app, ctx);
    crate::app::ui::widget_ids::configure_debug_options(ctx);
}

pub(super) fn render_frame(app: &mut ScratchpadApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    apply_deferred_layout_settings(app, ctx);
    let paint_started_at = Instant::now();
    paint_root_background(ui, app.state.app_settings.editor_background_color());
    record_frame_phase(FramePhase::Paint, paint_started_at.elapsed());
    let chrome_started_at = Instant::now();
    render_tab_chrome(app, ui);
    record_frame_phase(FramePhase::Chrome, chrome_started_at.elapsed());
    let active_surface_started_at = Instant::now();
    render_active_surface(app, ui);
    record_frame_phase(
        FramePhase::ActiveSurface,
        active_surface_started_at.elapsed(),
    );
    let dialogs_started_at = Instant::now();
    dialogs::show_startup_restore_conflict_modal(ctx, app);
    dialogs::show_pending_action_modal(ctx, app);
    dialogs::show_encoding_window(ctx, app);
    dialogs::show_text_history_window(ctx, app);
    dialogs::show_status_history_window(ctx, &mut app.state.dialogs, &app.state.status);
    record_frame_phase(FramePhase::Dialogs, dialogs_started_at.elapsed());
    let shortcuts_started_at = Instant::now();
    shortcuts::handle_shortcuts(app, ctx);
    record_frame_phase(FramePhase::Shortcuts, shortcuts_started_at.elapsed());
    let finish_started_at = Instant::now();
    show_window_after_first_frame(app, ctx);
    finish_frame_transitions(app, ctx);
    show_window_resize_cursor(ctx, platform_capabilities(app).allow_app_resize_grips);
    record_frame_phase(FramePhase::Finish, finish_started_at.elapsed());
}

fn platform_capabilities(app: &ScratchpadApp) -> platform::PlatformCapabilities {
    platform::capabilities(app.state.app_settings.platform_profile())
}

fn render_tab_chrome(app: &mut ScratchpadApp, ui: &mut egui::Ui) {
    if app.state.app_settings.tab_list_position() == TabListPosition::Top {
        tab_strip::show_header(ui, app);
    } else if platform_capabilities(app).allow_app_drag_regions {
        tab_strip::show_top_drag_bar(ui, app);
    }
    if app.state.app_settings.status_bar_visible() {
        status_bar::show_status_bar(ui, app);
    }
    tab_strip::show_bottom_tab_list(ui, app);
    tab_strip::show_vertical_tab_list(ui, app);
}

fn render_active_surface(app: &mut ScratchpadApp, ui: &mut egui::Ui) {
    match app.state.chrome.active_surface() {
        AppSurface::Workspace => editor_area::show_editor(ui, app),
        AppSurface::Settings => settings::show_page(ui, app),
    }
}

fn sync_window_title(app: &mut ScratchpadApp, ctx: &egui::Context) {
    let title = window_title(app);
    if app.state.current_window_title.as_ref() == Some(&title) {
        return;
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
    app.state.current_window_title = Some(title);
}

fn handle_dropped_files(app: &mut ScratchpadApp, ctx: &egui::Context) {
    let paths = ctx.input(|input| dropped_file_paths(&input.raw.dropped_files));
    if paths.is_empty() {
        return;
    }

    FileController::open_paths_async(app, paths);
}

fn show_window_after_first_frame(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if app.state.window_shown_after_first_frame {
        return;
    }
    if app.state.painted_frames_before_window_show < 2 {
        app.state.painted_frames_before_window_show += 1;
        if app.state.painted_frames_before_window_show == 2
            && app.state.app_settings.ui.window_state.maximized
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
        ctx.request_repaint();
        return;
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    app.state.window_shown_after_first_frame = true;
}

fn persist_with_error_status(app: &mut ScratchpadApp) -> bool {
    match crate::app::app_state::workspace::accessors::persist_session_now(app) {
        Ok(()) => true,
        Err(error) => {
            app.state.status.report_session_save_failed(error);
            false
        }
    }
}

pub(crate) fn begin_chrome_transition(app: &mut ScratchpadApp) {
    app.state.chrome.transition.begin();
}

pub(crate) fn begin_layout_transition(app: &mut ScratchpadApp) {
    begin_chrome_transition(app);
}

pub(crate) fn chrome_transition_active(app: &ScratchpadApp) -> bool {
    app.state.chrome.transition.is_active()
}

fn modal_callout_open(app: &ScratchpadApp) -> bool {
    app.state.dialogs.any_modal_open()
        || crate::app::app_state::workspace::accessors::pending_action(app).is_some()
        || restore_conflict::current_startup_restore_conflict(app).is_some()
}

fn finish_frame_transitions(app: &mut ScratchpadApp, ctx: &egui::Context) {
    app.state.chrome.transition.finish_frame();
    transition::set_chrome_transition_active(ctx, chrome_transition_active(app));
    if chrome_transition_active(app) {
        ctx.request_repaint();
    }
}

fn apply_deferred_layout_settings(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if ctx.current_pass_index() != 0 {
        return;
    }
    if let Some(visible) = app.state.chrome.take_pending_status_bar_visible() {
        crate::app::app_state::settings_controller::set_status_bar_visible(app, visible);
    }
}

pub(crate) fn estimated_tab_strip_width(app: &ScratchpadApp, spacing: f32) -> f32 {
    let tab_count = crate::app::app_state::workspace::display_tabs::total_tab_slots(app);
    if tab_count > 0 {
        (tab_count as f32 * crate::app::theme::TAB_BUTTON_WIDTH)
            + ((tab_count.saturating_sub(1)) as f32 * spacing)
    } else {
        0.0
    }
}

pub(crate) fn request_exit(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if app.state.close_in_progress {
        return;
    }

    app.record_window_state(ctx);
    if !persist_settings_with_error_status(app) {
        return;
    }

    if persist_with_error_status(app) {
        app.state.close_in_progress = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

fn persist_settings_with_error_status(app: &mut ScratchpadApp) -> bool {
    match crate::app::app_state::settings_state::persist_settings_now(app) {
        Ok(()) => true,
        Err(error) => {
            app.state.status.report_settings_save_failed(error);
            false
        }
    }
}

pub(crate) fn window_title(app: &ScratchpadApp) -> String {
    if crate::app::app_state::settings_state::showing_settings(app) {
        return "Settings - Scratchpad".to_owned();
    }

    if app.tab_manager.tabs.is_empty() {
        return "Scratchpad".to_owned();
    }

    let index = app
        .tab_manager
        .active_tab_index
        .min(app.tab_manager.tabs.len() - 1);
    let tab = &app.tab_manager.tabs[index];
    format!("{} - Scratchpad", tab.active_buffer().name)
}

fn sync_editor_fonts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    let selection = app.state.app_settings.editor_font_selection();
    if app.state.applied_editor_font.as_ref() == Some(&selection) {
        return;
    }

    if let Err(error) = fonts::apply_editor_fonts(ctx, &selection) {
        diagnostics::record_warning(
            "apply_editor_font",
            None,
            "app_state::frame",
            format!(
                "Editor font '{}' unavailable; using default fallback: {error}",
                selection.label()
            ),
        );
        app.state.status.set_warning_status_with_detail(
            crate::app::app_state::StatusDomain::Settings,
            format!(
                "Could not use editor font '{}'. The bundled fallback is in use.",
                selection.label()
            ),
            error.to_string(),
        );
    }
    clear_editor_layout_caches(app);
    app.state.applied_editor_font = Some(selection);
}

fn clear_editor_layout_caches(app: &mut ScratchpadApp) {
    for tab in app.tab_manager.tabs.as_mut_slice() {
        for view in tab.layout.views_mut() {
            view.layout_cache.clear();
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
