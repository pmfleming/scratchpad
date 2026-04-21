use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::{action_hover_bg, text_primary};
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_DOWN, ARROW_RIGHT, CARET_RIGHT, COPY, FLOPPY_DISK, FOLDER_OPEN, MINUS,
    PENCIL_SIMPLE_LINE, PLUS, TABS, TRANSLATE, TRAY, X, X_SQUARE,
};
use std::path::{Path, PathBuf};

const TAB_CONTEXT_MENU_WIDTH: f32 = 220.0;
const TAB_CONTEXT_SUBMENU_WIDTH: f32 = 176.0;
const TAB_CONTEXT_MENU_ROW_HEIGHT: f32 = 28.0;
const TAB_CONTEXT_MENU_CARET_WIDTH: f32 = 28.0;

pub(crate) fn attach_tab_context_menu(
    response: &egui::Response,
    app: &mut ScratchpadApp,
    slot_index: usize,
) {
    if response.secondary_clicked() {
        app.select_only_tab_slot(slot_index);
    }

    let workspace_index = app.workspace_index_for_slot(slot_index);
    let is_settings = app.tab_slot_is_settings(slot_index);
    let open_here_enabled = workspace_index.is_some();
    let rename_enabled = workspace_index.is_some();
    let save_enabled = workspace_index.is_some();
    let encoding_enabled = workspace_index.is_some();
    let path = tab_slot_path(app, slot_index);
    let copy_path_enabled = path.is_some();
    let reveal_enabled = path.is_some();
    let toggle_tab_list_label = if app.auto_hide_tab_list() {
        "Pin Tab List"
    } else {
        "Hide Tab List"
    };
    let toggle_tab_list_icon = if app.auto_hide_tab_list() {
        TRAY
    } else {
        MINUS
    };
    let close_direction_label = close_direction_label(app.tab_list_position());
    let close_direction_icon = close_direction_icon(app.tab_list_position());

    response.context_menu(|ui| {
        ui.set_min_width(TAB_CONTEXT_MENU_WIDTH);
        ui.set_max_width(TAB_CONTEXT_MENU_WIDTH);

        render_file_actions(
            ui,
            app,
            slot_index,
            workspace_index,
            open_here_enabled,
            rename_enabled,
            save_enabled,
        );

        ui.separator();

        if menu_button(
            ui,
            TAB_CONTEXT_MENU_WIDTH,
            toggle_tab_list_label,
            Some(toggle_tab_list_icon),
            true,
        ) {
            app.set_auto_hide_tab_list(!app.auto_hide_tab_list());
            ui.close();
        }

        ui.separator();

        render_location_actions(
            ui,
            app,
            slot_index,
            encoding_enabled,
            copy_path_enabled,
            reveal_enabled,
            path.as_deref(),
        );

        ui.separator();

        if render_close_actions(
            ui,
            app,
            slot_index,
            is_settings,
            close_direction_label,
            close_direction_icon,
        ) {
            ui.close();
        }
    });
}

fn render_file_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    workspace_index: Option<usize>,
    open_here_enabled: bool,
    rename_enabled: bool,
    save_enabled: bool,
) {
    if menu_button(ui, TAB_CONTEXT_MENU_WIDTH, "New Tab", Some(PLUS), true) {
        app.handle_command(AppCommand::NewTab);
        ui.close();
    }
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Open File Here",
        Some(FOLDER_OPEN),
        open_here_enabled,
    ) {
        activate_slot(app, slot_index);
        app.handle_command(AppCommand::OpenFileHere);
        ui.close();
    }
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Rename",
        Some(PENCIL_SIMPLE_LINE),
        rename_enabled,
    ) {
        if let Some(index) = workspace_index {
            app.begin_tab_rename(index);
        }
        ui.close();
    }
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Save",
        Some(FLOPPY_DISK),
        save_enabled,
    ) {
        if let Some(index) = workspace_index {
            app.save_file_at(index);
        }
        ui.close();
    }
}

fn render_location_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    encoding_enabled: bool,
    copy_path_enabled: bool,
    reveal_enabled: bool,
    path: Option<&Path>,
) {
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Encoding",
        Some(TRANSLATE),
        encoding_enabled,
    ) {
        activate_slot(app, slot_index);
        app.open_encoding_dialog();
        ui.close();
    }
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Copy Path",
        Some(COPY),
        copy_path_enabled,
    ) {
        if let Some(path) = path {
            ui.copy_text(path.display().to_string());
        }
        ui.close();
    }
    if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Reveal In Explorer",
        Some(FOLDER_OPEN),
        reveal_enabled,
    ) {
        if let Some(path) = path
            && let Err(error) = reveal_in_explorer(path)
        {
            app.set_warning_status(format!("Reveal in Explorer failed: {error}"));
        }
        ui.close();
    }
}

fn render_close_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    is_settings: bool,
    close_direction_label: &str,
    close_direction_icon: &str,
) -> bool {
    close_menu_row(
        ui,
        app,
        slot_index,
        is_settings,
        close_direction_label,
        close_direction_icon,
    )
}

fn menu_button(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    icon: Option<&str>,
    enabled: bool,
) -> bool {
    let text = match icon {
        Some(icon) => format!("{icon}  {label}"),
        None => label.to_owned(),
    };
    ui.scope(|ui| {
        apply_context_menu_row_hover_style(ui);
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(text).color(text_primary(ui)))
                .min_size(egui::vec2(width, TAB_CONTEXT_MENU_ROW_HEIGHT))
                .stroke(egui::Stroke::NONE),
        )
        .clicked()
    })
    .inner
}

fn close_menu_row(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    is_settings: bool,
    close_direction_label: &str,
    close_direction_icon: &str,
) -> bool {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        let close_clicked = ui
            .scope(|ui| {
                apply_context_menu_row_hover_style(ui);
                let response = ui.add(
                    egui::Button::new("")
                        .min_size(egui::vec2(
                            TAB_CONTEXT_MENU_WIDTH - TAB_CONTEXT_MENU_CARET_WIDTH,
                            TAB_CONTEXT_MENU_ROW_HEIGHT,
                        ))
                        .stroke(egui::Stroke::NONE),
                );
                ui.painter().text(
                    response.rect.left_center() + egui::vec2(12.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{X}  Close"),
                    egui::TextStyle::Button.resolve(ui.style()),
                    text_primary(ui),
                );
                response.clicked()
            })
            .inner;

        ui.scope(|ui| {
            apply_context_menu_row_hover_style(ui);
            let button =
                egui::Button::new(egui::RichText::new(CARET_RIGHT).color(text_primary(ui)))
                    .min_size(egui::vec2(
                        TAB_CONTEXT_MENU_CARET_WIDTH,
                        TAB_CONTEXT_MENU_ROW_HEIGHT,
                    ))
                    .stroke(egui::Stroke::NONE);

            egui::containers::menu::SubMenuButton::from_button(button).ui(ui, |ui| {
                ui.set_min_width(TAB_CONTEXT_SUBMENU_WIDTH);
                ui.set_max_width(TAB_CONTEXT_SUBMENU_WIDTH);

                if menu_button(
                    ui,
                    TAB_CONTEXT_SUBMENU_WIDTH,
                    "Close Others",
                    Some(TABS),
                    true,
                ) {
                    close_other_slots(app, slot_index);
                    ui.close();
                }
                if menu_button(
                    ui,
                    TAB_CONTEXT_SUBMENU_WIDTH,
                    close_direction_label,
                    Some(close_direction_icon),
                    true,
                ) {
                    close_slots_after(app, slot_index);
                    ui.close();
                }
                if menu_button(
                    ui,
                    TAB_CONTEXT_SUBMENU_WIDTH,
                    "Close Saved",
                    Some(FLOPPY_DISK),
                    true,
                ) {
                    close_saved_slots(app);
                    ui.close();
                }
                if menu_button(
                    ui,
                    TAB_CONTEXT_SUBMENU_WIDTH,
                    "Close All",
                    Some(X_SQUARE),
                    true,
                ) {
                    close_all_slots(app);
                    ui.close();
                }
            });
        });

        close_clicked
    })
    .inner
    .then(|| close_current_slot(app, slot_index, is_settings))
    .is_some()
}

fn apply_context_menu_row_hover_style(ui: &mut egui::Ui) {
    let hover_bg = action_hover_bg(ui);
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.hovered.bg_fill = hover_bg;
    visuals.widgets.hovered.weak_bg_fill = hover_bg;
    visuals.widgets.active.bg_fill = hover_bg;
    visuals.widgets.active.weak_bg_fill = hover_bg;
    visuals.widgets.open.bg_fill = hover_bg;
    visuals.widgets.open.weak_bg_fill = hover_bg;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.open.bg_stroke = egui::Stroke::NONE;
}

fn activate_slot(app: &mut ScratchpadApp, slot_index: usize) {
    if let Some(index) = app.workspace_index_for_slot(slot_index) {
        app.handle_command(AppCommand::ActivateTab { index });
    } else if app.tab_slot_is_settings(slot_index) {
        app.handle_command(AppCommand::OpenSettings);
    }
}

fn tab_slot_path(app: &ScratchpadApp, slot_index: usize) -> Option<PathBuf> {
    if let Some(index) = app.workspace_index_for_slot(slot_index) {
        return app
            .tabs()
            .get(index)
            .and_then(|tab| tab.active_buffer().path.clone());
    }

    app.tab_slot_is_settings(slot_index)
        .then(|| app.settings_path().to_path_buf())
}

fn close_direction_label(position: TabListPosition) -> &'static str {
    if position.is_vertical() {
        "Close Down"
    } else {
        "Close To The Right"
    }
}

fn close_direction_icon(position: TabListPosition) -> &'static str {
    if position.is_vertical() {
        ARROW_DOWN
    } else {
        ARROW_RIGHT
    }
}

fn close_current_slot(app: &mut ScratchpadApp, slot_index: usize, is_settings: bool) {
    if is_settings {
        app.close_settings();
    } else if let Some(index) = app.workspace_index_for_slot(slot_index) {
        app.handle_command(AppCommand::RequestCloseTab { index });
    }
}

fn close_other_slots(app: &mut ScratchpadApp, current_slot: usize) {
    let slots = tab_slots(app)
        .into_iter()
        .filter(|slot_index| *slot_index != current_slot)
        .collect::<Vec<_>>();
    close_display_slots(app, slots, CloseDisplayTabs::SkipDirty, "Close Others");
}

fn close_slots_after(app: &mut ScratchpadApp, current_slot: usize) {
    let slots = ((current_slot + 1)..app.total_tab_slots()).collect::<Vec<_>>();
    close_display_slots(app, slots, CloseDisplayTabs::SkipDirty, "Close tabs");
}

fn close_saved_slots(app: &mut ScratchpadApp) {
    let slots = tab_slots(app);
    close_display_slots(app, slots, CloseDisplayTabs::SavedOnly, "Close Saved");
}

fn close_all_slots(app: &mut ScratchpadApp) {
    let slots = tab_slots(app);
    close_display_slots(app, slots, CloseDisplayTabs::SkipDirty, "Close All");
}

fn tab_slots(app: &ScratchpadApp) -> Vec<usize> {
    (0..app.total_tab_slots()).collect()
}

#[derive(Clone, Copy)]
enum CloseDisplayTabs {
    SkipDirty,
    SavedOnly,
}

fn close_display_slots(
    app: &mut ScratchpadApp,
    slots: Vec<usize>,
    mode: CloseDisplayTabs,
    action_name: &str,
) {
    let (mut workspace_indices, close_settings, skipped_dirty) =
        collect_close_targets(app, slots, mode);

    workspace_indices.sort_unstable();
    workspace_indices.dedup();

    let mut closed_count = 0usize;
    for index in workspace_indices.into_iter().rev() {
        if index < app.tabs().len() {
            app.perform_close_tab_no_persist(index);
            closed_count += 1;
        }
    }

    if close_settings {
        app.close_settings();
    }

    if closed_count > 0 || close_settings {
        let _ = app.persist_session_now();
    }

    if skipped_dirty > 0 {
        app.set_warning_status(format!(
            "{action_name} skipped {skipped_dirty} tab(s) with unsaved changes."
        ));
    }
}

fn collect_close_targets(
    app: &ScratchpadApp,
    slots: Vec<usize>,
    mode: CloseDisplayTabs,
) -> (Vec<usize>, bool, usize) {
    let mut workspace_indices = Vec::new();
    let mut close_settings = false;
    let mut skipped_dirty = 0usize;

    for slot_index in slots {
        if app.tab_slot_is_settings(slot_index) {
            close_settings |= matches!(mode, CloseDisplayTabs::SkipDirty);
            continue;
        }

        let Some(index) = app.workspace_index_for_slot(slot_index) else {
            continue;
        };
        let is_dirty = app
            .tabs()
            .get(index)
            .is_some_and(|tab| tab.buffers().any(|buffer| buffer.is_dirty));
        if should_close_slot(mode, is_dirty) {
            workspace_indices.push(index);
        } else if matches!(mode, CloseDisplayTabs::SkipDirty) && is_dirty {
            skipped_dirty += 1;
        }
    }

    (workspace_indices, close_settings, skipped_dirty)
}

fn should_close_slot(mode: CloseDisplayTabs, is_dirty: bool) -> bool {
    match mode {
        CloseDisplayTabs::SkipDirty => !is_dirty,
        CloseDisplayTabs::SavedOnly => !is_dirty,
    }
}

#[cfg(target_os = "windows")]
fn reveal_in_explorer(path: &Path) -> std::io::Result<()> {
    use std::ffi::OsString;
    use std::process::Command;

    let mut select_arg = OsString::from("/select,");
    select_arg.push(path);
    Command::new("explorer.exe")
        .arg(select_arg)
        .spawn()
        .map(|_| ())
}

#[cfg(not(target_os = "windows"))]
fn reveal_in_explorer(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Reveal in Explorer is only available on Windows.",
    ))
}
