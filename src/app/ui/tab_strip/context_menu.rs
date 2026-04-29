use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::{action_hover_bg, text_primary};
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, ARROW_UP, CARET_RIGHT, COPY, FLOPPY_DISK, FOLDER_OPEN,
    MINUS, PENCIL_SIMPLE_LINE, PLUS, TABS, TRANSLATE, TRAY, X, X_SQUARE,
};
use std::path::{Path, PathBuf};

mod close;

const TAB_CONTEXT_MENU_WIDTH: f32 = 220.0;
const TAB_CONTEXT_SUBMENU_WIDTH: f32 = 176.0;
const TAB_CONTEXT_MENU_ROW_HEIGHT: f32 = 28.0;
const TAB_CONTEXT_MENU_CARET_WIDTH: f32 = 28.0;

struct TabContextMenuState {
    workspace_index: Option<usize>,
    is_settings: bool,
    path: Option<PathBuf>,
    toggle_tab_list_label: &'static str,
    toggle_tab_list_icon: &'static str,
    close_direction_label: &'static str,
    close_direction_icon: &'static str,
}

pub(crate) fn attach_tab_context_menu(
    response: &egui::Response,
    app: &mut ScratchpadApp,
    slot_index: usize,
) {
    if response.secondary_clicked() {
        app.select_only_tab_slot(slot_index);
    }

    let menu_state = TabContextMenuState::new(app, slot_index);

    response.context_menu(|ui| {
        ui.set_min_width(TAB_CONTEXT_MENU_WIDTH);
        ui.set_max_width(TAB_CONTEXT_MENU_WIDTH);

        render_file_actions(
            ui,
            app,
            slot_index,
            menu_state.workspace_index,
            menu_state.workspace_index.is_some(),
            menu_state.workspace_index.is_some(),
            menu_state.workspace_index.is_some(),
        );

        ui.separator();

        if render_tab_list_actions(
            ui,
            app,
            menu_state.toggle_tab_list_label,
            menu_state.toggle_tab_list_icon,
        ) {
            app.set_auto_hide_tab_list(!app.auto_hide_tab_list());
            ui.close();
        }

        ui.separator();

        render_location_actions(
            ui,
            app,
            slot_index,
            menu_state.workspace_index.is_some(),
            menu_state.path.is_some(),
            menu_state.path.is_some(),
            menu_state.path.as_deref(),
        );

        ui.separator();

        if render_close_actions(
            ui,
            app,
            slot_index,
            menu_state.is_settings,
            menu_state.close_direction_label,
            menu_state.close_direction_icon,
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
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(text).color(text_primary(ui)))
                .min_size(egui::vec2(width, TAB_CONTEXT_MENU_ROW_HEIGHT))
                .stroke(egui::Stroke::NONE),
        )
        .clicked()
    })
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

        let close_clicked = render_close_primary_button(ui);
        render_close_submenu(
            ui,
            app,
            slot_index,
            close_direction_label,
            close_direction_icon,
        );

        close_clicked
    })
    .inner
    .then(|| close::close_current_slot(app, slot_index, is_settings))
    .is_some()
}

impl TabContextMenuState {
    fn new(app: &ScratchpadApp, slot_index: usize) -> Self {
        let workspace_index = app.workspace_index_for_slot(slot_index);
        let auto_hide = app.auto_hide_tab_list();
        Self {
            workspace_index,
            is_settings: app.tab_slot_is_settings(slot_index),
            path: tab_slot_path(app, slot_index),
            toggle_tab_list_label: if auto_hide {
                "Pin Tab List"
            } else {
                "Hide Tab List"
            },
            toggle_tab_list_icon: if auto_hide { TRAY } else { MINUS },
            close_direction_label: close_direction_label(app.tab_list_position()),
            close_direction_icon: close_direction_icon(app.tab_list_position()),
        }
    }
}

fn render_tab_list_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    toggle_label: &str,
    toggle_icon: &str,
) -> bool {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        let toggle_clicked = render_tab_list_primary_button(ui, toggle_label, toggle_icon);
        render_tab_list_submenu(ui, app);

        toggle_clicked
    })
    .inner
}

fn render_close_primary_button(ui: &mut egui::Ui) -> bool {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
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
}

fn render_tab_list_primary_button(ui: &mut egui::Ui, label: &str, icon: &str) -> bool {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
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
            format!("{icon}  {label}"),
            egui::TextStyle::Button.resolve(ui.style()),
            text_primary(ui),
        );
        response.clicked()
    })
}

fn render_tab_list_submenu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let button = egui::Button::new(egui::RichText::new(CARET_RIGHT).color(text_primary(ui)))
            .min_size(egui::vec2(
                TAB_CONTEXT_MENU_CARET_WIDTH,
                TAB_CONTEXT_MENU_ROW_HEIGHT,
            ))
            .stroke(egui::Stroke::NONE);

        egui::containers::menu::SubMenuButton::from_button(button).ui(ui, |ui| {
            ui.set_min_width(TAB_CONTEXT_SUBMENU_WIDTH);
            ui.set_max_width(TAB_CONTEXT_SUBMENU_WIDTH);

            for position in [
                TabListPosition::Top,
                TabListPosition::Bottom,
                TabListPosition::Left,
                TabListPosition::Right,
            ] {
                if menu_button(
                    ui,
                    TAB_CONTEXT_SUBMENU_WIDTH,
                    tab_list_position_label(position),
                    Some(tab_list_position_icon(position)),
                    true,
                ) {
                    app.set_tab_list_position(position);
                    ui.close();
                }
            }
        });
    });
}

fn render_close_submenu(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    close_direction_label: &str,
    close_direction_icon: &str,
) {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let button = egui::Button::new(egui::RichText::new(CARET_RIGHT).color(text_primary(ui)))
            .min_size(egui::vec2(
                TAB_CONTEXT_MENU_CARET_WIDTH,
                TAB_CONTEXT_MENU_ROW_HEIGHT,
            ))
            .stroke(egui::Stroke::NONE);

        egui::containers::menu::SubMenuButton::from_button(button).ui(ui, |ui| {
            ui.set_min_width(TAB_CONTEXT_SUBMENU_WIDTH);
            ui.set_max_width(TAB_CONTEXT_SUBMENU_WIDTH);

            for (label, icon, action) in [
                ("Close Others", TABS, TabCloseAction::Others),
                (
                    close_direction_label,
                    close_direction_icon,
                    TabCloseAction::After,
                ),
                ("Close Saved", FLOPPY_DISK, TabCloseAction::Saved),
                ("Close All", X_SQUARE, TabCloseAction::All),
            ] {
                if menu_button(ui, TAB_CONTEXT_SUBMENU_WIDTH, label, Some(icon), true) {
                    match action {
                        TabCloseAction::Others => close::close_other_slots(app, slot_index),
                        TabCloseAction::After => close::close_slots_after(app, slot_index),
                        TabCloseAction::Saved => close::close_saved_slots(app),
                        TabCloseAction::All => close::close_all_slots(app),
                    }
                    ui.close();
                }
            }
        });
    });
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

fn with_visual_overrides<R>(
    ui: &mut egui::Ui,
    configure: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let previous_visuals = ui.visuals().clone();
    configure(ui);
    let result = add_contents(ui);
    *ui.visuals_mut() = previous_visuals;
    result
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

fn tab_list_position_label(position: TabListPosition) -> &'static str {
    match position {
        TabListPosition::Top => "Top",
        TabListPosition::Bottom => "Bottom",
        TabListPosition::Left => "Left",
        TabListPosition::Right => "Right",
    }
}

fn tab_list_position_icon(position: TabListPosition) -> &'static str {
    match position {
        TabListPosition::Top => ARROW_UP,
        TabListPosition::Bottom => ARROW_DOWN,
        TabListPosition::Left => ARROW_LEFT,
        TabListPosition::Right => ARROW_RIGHT,
    }
}

enum TabCloseAction {
    Others,
    After,
    Saved,
    All,
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
