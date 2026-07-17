use super::{
    AppSettings, AppThemeMode, EditorAppearanceSource, FileController, FileOpenDisposition,
    IndentationStyle, NewTabPlacement, ScratchpadApp, StartupSessionBehavior, TabDisplayMode,
    TabListPosition, sanitize_tab_list_auto_hide_delay_seconds,
};
use crate::app::app_state::{AppSurface, workspace::accessors as workspace_accessors};
use crate::app::domain::TextHistoryBudget;
use crate::app::fonts::EditorFontPreset;
use eframe::egui;
use model::{SettingEffects, replace_if_changed};
use std::time::Instant;

mod model;
#[cfg(test)]
mod tests;

pub(super) fn persist_settings_or_error(app: &mut ScratchpadApp) {
    if let Err(error) = crate::app::app_state::settings_state::persist_settings_now(app) {
        app.state.status.report_settings_save_failed(error);
    }
}

fn apply_setting_effects(app: &mut ScratchpadApp, changed: bool, effects: SettingEffects) {
    if !changed {
        return;
    }
    if effects.invalidate_font {
        app.state.applied_editor_font = None;
    }
    if effects.invalidate_theme {
        app.state.applied_theme_mode = None;
    }
    if effects.relayout {
        crate::app::app_state::frame::begin_layout_transition(app);
    }
    if effects.apply_history_budget {
        app.apply_history_budget_to_open_buffers();
    }
    persist_settings_or_error(app);
}

fn reset_tab_list_visibility_state(app: &mut ScratchpadApp, keep_open: bool) {
    app.state.chrome.vertical_tabs.reset_visibility(keep_open);
}

fn clear_tab_list_hide_deadline(app: &mut ScratchpadApp) {
    app.state.chrome.vertical_tabs.clear_hide_deadline();
}

fn set_tab_list_width(app: &mut ScratchpadApp, width: f32) {
    app.state.app_settings.workspace.tab_list_width = width;
    apply_setting_effects(app, true, SettingEffects::RELAYOUT);
}

fn set_settings_surface(app: &mut ScratchpadApp, surface: AppSurface, open: bool) -> bool {
    let changed = crate::app::app_state::settings_state::settings_tab_open(app) != open;
    app.state.settings_tab_index = app
        .state
        .settings_tab_index
        .min(app.tab_manager.tabs.as_slice().len());
    app.state.app_settings.ui.settings_tab_open = open;
    let surface_changed = app.state.chrome.set_active_surface(surface);
    if surface_changed {
        app.tab_manager.mark_session_dirty();
    }
    crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);
    app.tab_manager.pending_scroll_to_active = true;
    changed
}

pub(crate) fn set_editor_appearance_source(
    app: &mut ScratchpadApp,
    source: EditorAppearanceSource,
) {
    let changed = app.state.app_settings.set_editor_appearance_source(source);
    apply_setting_effects(app, changed, SettingEffects::FONT_AND_THEME);
}

pub(crate) fn set_font_size(app: &mut ScratchpadApp, font_size: f32) {
    let changed = app.state.app_settings.set_font_size(font_size);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_editor_font(app: &mut ScratchpadApp, editor_font: EditorFontPreset) {
    let changed = app.state.app_settings.set_editor_font(editor_font);
    apply_setting_effects(app, changed, SettingEffects::FONT);
}

pub(crate) fn set_word_wrap(app: &mut ScratchpadApp, enabled: bool) {
    let changed = app.state.app_settings.set_word_wrap(enabled);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_editor_gutter(app: &mut ScratchpadApp, gutter: u8) {
    let changed = app.state.app_settings.set_editor_gutter(gutter);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_editor_tab_width(app: &mut ScratchpadApp, tab_width: u8) {
    let changed = app.state.app_settings.set_editor_tab_width(tab_width);
    apply_setting_effects(app, changed, SettingEffects::FONT);
}

pub(crate) fn set_indentation_style(app: &mut ScratchpadApp, style: IndentationStyle) {
    let changed = app.state.app_settings.set_indentation_style(style);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_tab_display(app: &mut ScratchpadApp, mode: TabDisplayMode) {
    let changed = app.state.app_settings.set_tab_display(mode);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn apply_theme_mode_preset(
    app: &mut ScratchpadApp,
    theme_mode: AppThemeMode,
    system_theme: Option<egui::Theme>,
) {
    let changed = app
        .state
        .app_settings
        .apply_theme_mode_preset(theme_mode, system_theme);
    apply_setting_effects(app, changed, SettingEffects::THEME);
}

pub(crate) fn set_editor_text_color(app: &mut ScratchpadApp, color: egui::Color32) {
    let changed = app.state.app_settings.set_editor_text_color(color);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_editor_background_color(app: &mut ScratchpadApp, color: egui::Color32) {
    let changed = app.state.app_settings.set_editor_background_color(color);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_editor_text_highlight_color(app: &mut ScratchpadApp, color: egui::Color32) {
    let changed = app
        .state
        .app_settings
        .set_editor_text_highlight_color(color);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_tab_list_position(app: &mut ScratchpadApp, position: TabListPosition) {
    if app.state.app_settings.workspace.tab_list_position == position {
        return;
    }

    app.state.app_settings.workspace.tab_list_position = position;
    reset_tab_list_visibility_state(app, false);
    if position.is_vertical() {
        app.state.overflow_popup_open = false;
    }
    app.tab_manager.pending_scroll_to_active = true;
    apply_setting_effects(app, true, SettingEffects::RELAYOUT);
}

pub(crate) fn set_file_open_disposition(app: &mut ScratchpadApp, disposition: FileOpenDisposition) {
    let changed = app
        .state
        .app_settings
        .set_file_open_disposition(disposition);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_new_tab_placement(app: &mut ScratchpadApp, placement: NewTabPlacement) {
    let changed = app.state.app_settings.set_new_tab_placement(placement);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_startup_session_behavior(
    app: &mut ScratchpadApp,
    behavior: StartupSessionBehavior,
) {
    let changed = app
        .state
        .app_settings
        .set_startup_session_behavior(behavior);
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_auto_hide_tab_list(app: &mut ScratchpadApp, enabled: bool) {
    if app.state.app_settings.workspace.auto_hide_tab_list == enabled {
        return;
    }

    app.state.app_settings.workspace.auto_hide_tab_list = enabled;
    reset_tab_list_visibility_state(app, enabled && app.state.chrome.vertical_tabs_open());
    apply_setting_effects(app, true, SettingEffects::RELAYOUT);
}

pub(crate) fn set_tab_list_auto_hide_delay_seconds(app: &mut ScratchpadApp, seconds: f32) {
    let next = sanitize_tab_list_auto_hide_delay_seconds(seconds);
    if (app
        .state
        .app_settings
        .workspace
        .tab_list_auto_hide_delay_seconds
        - next)
        .abs()
        < f32::EPSILON
    {
        return;
    }

    app.state
        .app_settings
        .workspace
        .tab_list_auto_hide_delay_seconds = next;
    clear_tab_list_hide_deadline(app);
    persist_settings_or_error(app);
}

pub(crate) fn set_recent_files_enabled(app: &mut ScratchpadApp, enabled: bool) {
    let changed = replace_if_changed(
        &mut app.state.app_settings.workspace.recent_files_enabled,
        enabled,
    );
    apply_setting_effects(app, changed, SettingEffects::PERSIST_ONLY);
}

pub(crate) fn set_status_bar_visible(app: &mut ScratchpadApp, visible: bool) {
    if app.state.app_settings.ui.status_bar_visible == visible {
        app.state.chrome.clear_pending_status_bar_visible();
        return;
    }

    app.state.app_settings.ui.status_bar_visible = visible;
    apply_setting_effects(app, true, SettingEffects::RELAYOUT);
}

pub(crate) fn defer_status_bar_visible(
    app: &mut ScratchpadApp,
    visible: bool,
    ctx: &egui::Context,
) {
    if app
        .state
        .chrome
        .defer_status_bar_visible(app.state.app_settings.ui.status_bar_visible, visible)
    {
        ctx.request_repaint();
    }
}

pub(crate) fn set_history_budget(app: &mut ScratchpadApp, budget: TextHistoryBudget) {
    let changed = app.state.app_settings.set_history_budget(budget);
    apply_setting_effects(app, changed, SettingEffects::HISTORY_BUDGET);
}

pub(crate) fn reset_history_budget_to_auto(app: &mut ScratchpadApp) {
    app.state.app_settings.reset_history_budget_to_auto();
    apply_setting_effects(app, true, SettingEffects::HISTORY_BUDGET);
}

pub(crate) fn set_tab_list_width_from_layout(app: &mut ScratchpadApp, width: f32) {
    let next = width.clamp(
        ScratchpadApp::VERTICAL_TAB_LIST_MIN_WIDTH,
        ScratchpadApp::VERTICAL_TAB_LIST_MAX_WIDTH,
    );
    if (app.state.app_settings.workspace.tab_list_width - next).abs() < 1.0 {
        return;
    }

    set_tab_list_width(app, next);
}

pub(crate) fn open_settings(app: &mut ScratchpadApp) {
    open_settings_with_tab_selection(app, false);
}

pub(crate) fn open_settings_preserving_tab_selection(app: &mut ScratchpadApp) {
    open_settings_with_tab_selection(app, true);
}

fn open_settings_with_tab_selection(app: &mut ScratchpadApp, preserve_tab_selection: bool) {
    app.reload_settings_before_workspace_change();
    crate::app::app_state::frame::begin_layout_transition(app);
    if !crate::app::app_state::settings_state::settings_tab_open(app) {
        app.state.settings_preview_quote_index = (app.state.settings_preview_quote_index + 1)
            % crate::app::ui::settings::PREVIEW_QUOTES.len();
    }
    if set_settings_surface(app, AppSurface::Settings, true) {
        persist_settings_or_error(app);
    }
    if !preserve_tab_selection {
        crate::app::app_state::workspace::display_tabs::select_only_tab_slot(
            app,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app),
        );
    }
}

pub(crate) fn open_settings_file_tab(app: &mut ScratchpadApp) {
    let path = crate::app::app_state::settings_state::settings_path(app).to_path_buf();
    activate_workspace_surface(app);
    FileController::open_paths_async(app, vec![path]);
}

pub(crate) fn close_settings(app: &mut ScratchpadApp) {
    crate::app::app_state::frame::begin_layout_transition(app);
    if set_settings_surface(app, AppSurface::Workspace, false) {
        persist_settings_or_error(app);
    }
    crate::app::app_state::workspace::display_tabs::select_only_tab_slot(
        app,
        crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app),
    );
    workspace_accessors::request_focus_for_active_view(app);
}

pub(crate) fn reset_settings_to_defaults(app: &mut ScratchpadApp) {
    app.initialize_default_workspace_tabs();
    crate::app::app_state::settings_state::apply_settings(app, AppSettings::default());
    if crate::app::app_state::settings_state::settings_tab_open(app) {
        app.state.chrome.set_active_surface(AppSurface::Settings);
    }
    app.state.applied_editor_font = None;
    crate::app::app_state::workspace::display_tabs::select_only_tab_slot(
        app,
        crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app),
    );
    let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
    match crate::app::app_state::settings_state::persist_settings_now(app) {
        Ok(()) => app.state.status.set_info_status_in_domain(
            crate::app::app_state::StatusDomain::Settings,
            "Settings reset to defaults.",
        ),
        Err(error) => app.state.status.report_settings_save_failed(error),
    }
}

pub(crate) fn activate_workspace_surface(app: &mut ScratchpadApp) {
    if app.state.chrome.active_surface() == AppSurface::Workspace {
        return;
    }
    if app.state.chrome.activate_workspace_surface() {
        app.tab_manager.mark_session_dirty();
    }
}

pub(crate) fn keep_tab_list_open(app: &mut ScratchpadApp) {
    reset_tab_list_visibility_state(app, true);
}

pub(crate) fn delay_tab_list_hide(app: &mut ScratchpadApp, now: Instant) {
    app.state
        .chrome
        .vertical_tabs
        .delay_hide(now, app.state.app_settings.tab_list_auto_hide_delay());
}

pub(crate) fn close_tab_list(app: &mut ScratchpadApp) {
    reset_tab_list_visibility_state(app, false);
}

pub(crate) fn toggle_tab_list(app: &mut ScratchpadApp) {
    if !app.state.app_settings.auto_hide_tab_list() {
        app.state.app_settings.workspace.auto_hide_tab_list = true;
        reset_tab_list_visibility_state(app, false);
        crate::app::app_state::frame::begin_layout_transition(app);
        persist_settings_or_error(app);
        app.state.status.set_info_status_in_domain(
            crate::app::app_state::StatusDomain::Layout,
            "Tab list hidden. Press Ctrl+Alt+B to show it.",
        );
        return;
    }

    let open = !app.state.chrome.vertical_tabs_open();
    reset_tab_list_visibility_state(app, open);
    crate::app::app_state::frame::begin_layout_transition(app);
    app.state.status.set_info_status_in_domain(
        crate::app::app_state::StatusDomain::Layout,
        if open {
            "Tab list shown."
        } else {
            "Tab list hidden. Press Ctrl+Alt+B to show it."
        },
    );
}
