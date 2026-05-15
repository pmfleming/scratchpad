use crate::app::app_state::{
    PendingTabContextMenu, ScratchpadApp, StatusDomain, frame,
    workspace::accessors as workspace_accessors,
};
use crate::app::commands::{AppCommand, FileCommand, SettingsCommand, WorkspaceCommand};
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::FileOpenDisposition;
use crate::app::ui::widget_ids;
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_SQUARE_IN, COPY, FILE_PLUS, FLOPPY_DISK, FOLDER_OPEN, MINUS, PENCIL_SIMPLE_LINE, PLUS,
    TABS, TRANSLATE, TRAY, X, X_SQUARE,
};
use std::path::{Path, PathBuf};

mod close;
mod menu_ui;
mod order;

use self::menu_ui::{
    OPEN_DISPOSITION_BUTTON_SIZE, OPEN_FILE_SUBMENU_WIDTH as TAB_CONTEXT_OPEN_FILE_SUBMENU_WIDTH,
    SUBMENU_WIDTH as TAB_CONTEXT_SUBMENU_WIDTH, WIDTH as TAB_CONTEXT_MENU_WIDTH,
    close_direction_icon, close_direction_label, menu_button, open_disposition_button,
    primary_menu_button, primary_menu_button_enabled, recent_file_button, submenu_button,
    submenu_button_sized,
};

struct TabContextMenuState {
    workspace_index: Option<usize>,
    is_settings: bool,
    path: Option<PathBuf>,
    toggle_tab_list_label: &'static str,
    toggle_tab_list_icon: &'static str,
    close_direction_label: &'static str,
    close_direction_icon: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabContextClick {
    None,
    Secondary,
}

impl TabContextClick {
    pub(crate) fn secondary_clicked(self) -> bool {
        self == Self::Secondary
    }
}

pub(crate) fn attach_tab_context_menu(
    response: &egui::Response,
    app: &mut ScratchpadApp,
    slot_index: usize,
) -> TabContextClick {
    attach_tab_context_menu_impl(response, app, slot_index, true)
}

fn attach_tab_context_menu_impl(
    response: &egui::Response,
    app: &mut ScratchpadApp,
    slot_index: usize,
    allow_pending_tab_popup: bool,
) -> TabContextClick {
    let secondary_clicked = response.secondary_clicked();
    if secondary_clicked {
        crate::app::app_state::workspace::display_tabs::select_only_tab_slot(app, slot_index);
        if allow_pending_tab_popup {
            app.state
                .dialogs
                .open_pending_tab_context_menu(PendingTabContextMenu {
                    slot_index,
                    click_x: response
                        .interact_pointer_pos()
                        .map_or(response.rect.left(), |pos| pos.x),
                    click_y: response.rect.max.y,
                    open: true,
                });
        }
    }

    let menu_state = TabContextMenuState::new(app, slot_index);
    let pending_menu = app
        .state
        .dialogs
        .pending_tab_context_menu_for_slot(slot_index);

    if allow_pending_tab_popup && let Some(mut pending) = pending_menu {
        egui::Popup::new(
            widget_ids::root_id(("tab_context.popup", slot_index)),
            response.ctx.clone(),
            response.rect,
            response.layer_id,
        )
        .kind(egui::PopupKind::Menu)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .width(TAB_CONTEXT_MENU_WIDTH)
        .anchor(tab_context_menu_anchor(response, pending))
        .align(egui::RectAlign::BOTTOM_START)
        .align_alternatives(&[egui::RectAlign::BOTTOM_END])
        .open_bool(&mut pending.open)
        .show(|ui| render_tab_context_menu(ui, app, slot_index, &menu_state));

        app.state.dialogs.store_pending_tab_context_menu(pending);
        return if secondary_clicked {
            TabContextClick::Secondary
        } else {
            TabContextClick::None
        };
    }

    response.context_menu(|ui| render_tab_context_menu(ui, app, slot_index, &menu_state));

    if secondary_clicked {
        TabContextClick::Secondary
    } else {
        TabContextClick::None
    }
}

pub(crate) fn attach_tab_list_context_menu(response: &egui::Response, app: &mut ScratchpadApp) {
    let _ = attach_tab_context_menu_impl(
        response,
        app,
        crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app),
        false,
    );
}

fn tab_context_menu_anchor(
    _response: &egui::Response,
    pending: PendingTabContextMenu,
) -> egui::Pos2 {
    egui::pos2(pending.click_x, pending.click_y)
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
        app.handle_command(AppCommand::Workspace(WorkspaceCommand::NewTab));
        ui.close();
    }
    if app.state.app_settings.recent_files_enabled() {
        render_open_file_actions(ui, app, slot_index, open_here_enabled);
    } else if menu_button(
        ui,
        TAB_CONTEXT_MENU_WIDTH,
        "Open File Here",
        Some(FOLDER_OPEN),
        open_here_enabled,
    ) {
        activate_slot(app, slot_index);
        app.handle_command(AppCommand::File(FileCommand::OpenFileHere));
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
            workspace_accessors::begin_tab_rename(app, index);
        }
        ui.close();
    }
    render_save_actions(ui, app, workspace_index, save_enabled);
}

fn render_open_file_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    open_enabled: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        if primary_menu_button_enabled(
            ui,
            "tab_context.open_file_primary",
            "Open File",
            FOLDER_OPEN,
            open_enabled,
        ) {
            activate_slot(app, slot_index);
            app.handle_command(AppCommand::File(FileCommand::OpenFile));
            ui.close();
        }
        render_open_file_submenu(ui, app, slot_index);
    });
}

fn render_open_file_submenu(ui: &mut egui::Ui, app: &mut ScratchpadApp, slot_index: usize) {
    submenu_button_sized(
        ui,
        "tab_context.open_file_caret",
        TAB_CONTEXT_OPEN_FILE_SUBMENU_WIDTH,
        |ui| {
            render_open_file_disposition_buttons(ui, app);
            ui.separator();
            render_recently_closed_files(ui, app, slot_index);
        },
    );
}

fn render_open_file_disposition_buttons(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let current = app.state.app_settings.file_open_disposition();
    let spacing = 6.0;
    let group_width = OPEN_DISPOSITION_BUTTON_SIZE.x * 2.0 + spacing;
    let leading_space = (TAB_CONTEXT_OPEN_FILE_SUBMENU_WIDTH - group_width).max(0.0) * 0.5;
    ui.horizontal(|ui| {
        ui.add_space(leading_space);
        ui.spacing_mut().item_spacing.x = spacing;
        if open_disposition_button(
            ui,
            "tab_context.open_file.new_tab",
            FILE_PLUS,
            "Open in new tab",
            matches!(current, FileOpenDisposition::NewTab),
        ) {
            crate::app::app_state::settings_controller::set_file_open_disposition(
                app,
                FileOpenDisposition::NewTab,
            );
        }
        if open_disposition_button(
            ui,
            "tab_context.open_file.current_tab",
            ARROW_SQUARE_IN,
            "Open in current tab",
            matches!(current, FileOpenDisposition::CurrentTab),
        ) {
            crate::app::app_state::settings_controller::set_file_open_disposition(
                app,
                FileOpenDisposition::CurrentTab,
            );
        }
    });
}

fn render_recently_closed_files(ui: &mut egui::Ui, app: &mut ScratchpadApp, slot_index: usize) {
    let paths = app
        .state
        .recently_closed_files
        .iter()
        .take(crate::app::app_state::RECENTLY_CLOSED_FILE_LIMIT)
        .cloned()
        .collect::<Vec<_>>();

    if paths.is_empty() {
        let _ = menu_button(
            ui,
            TAB_CONTEXT_OPEN_FILE_SUBMENU_WIDTH,
            "No Recent Files",
            None,
            false,
        );
        return;
    }

    for path in paths {
        if recent_file_button(
            ui,
            ("tab_context.recently_closed_file", path.clone()),
            TAB_CONTEXT_OPEN_FILE_SUBMENU_WIDTH,
            &path,
        ) {
            activate_slot(app, slot_index);
            open_recent_file(app, path);
            ui.close();
        }
    }
}

fn open_recent_file(app: &mut ScratchpadApp, path: PathBuf) {
    match app.state.app_settings.file_open_disposition() {
        FileOpenDisposition::NewTab => FileController::open_paths_async(app, vec![path]),
        FileOpenDisposition::CurrentTab => {
            FileController::open_external_paths_here_async(app, vec![path])
        }
    }
}

fn render_tab_context_menu(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    menu_state: &TabContextMenuState,
) {
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

    if order::render_tab_list_actions(
        ui,
        app,
        menu_state.toggle_tab_list_label,
        menu_state.toggle_tab_list_icon,
    ) {
        crate::app::app_state::settings_controller::set_auto_hide_tab_list(
            app,
            !app.state.app_settings.auto_hide_tab_list(),
        );
        ui.close();
    }
    order::render_tab_order_submenu(ui, app);

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
        frame::open_encoding_dialog(app);
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
            app.state.status.set_warning_status_with_detail(
                StatusDomain::File,
                "Could not reveal this file in Explorer.",
                error.to_string(),
            );
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

fn render_save_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    workspace_index: Option<usize>,
    save_enabled: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        if primary_menu_button_enabled(
            ui,
            "tab_context.save_primary",
            "Save",
            FLOPPY_DISK,
            save_enabled,
        ) {
            if let Some(index) = workspace_index {
                crate::app::app_state::workspace_controller::save_file_at(app, index);
            }
            ui.close();
        }
        render_save_submenu(ui, app, save_enabled);
    });
}

fn render_save_submenu(ui: &mut egui::Ui, app: &mut ScratchpadApp, save_enabled: bool) {
    submenu_button(ui, "tab_context.save_caret", |ui| {
        if menu_button(
            ui,
            TAB_CONTEXT_SUBMENU_WIDTH,
            "Save All",
            Some(FLOPPY_DISK),
            save_enabled,
        ) {
            app.handle_command(AppCommand::File(FileCommand::SaveAllFiles));
            ui.close();
        }
    });
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
        let workspace_index =
            crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(
                app, slot_index,
            );
        let auto_hide = app.state.app_settings.auto_hide_tab_list();
        Self {
            workspace_index,
            is_settings: crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
                app, slot_index,
            ),
            path: tab_slot_path(app, slot_index),
            toggle_tab_list_label: if auto_hide {
                "Pin Tab List"
            } else {
                "Hide Tab List"
            },
            toggle_tab_list_icon: if auto_hide { TRAY } else { MINUS },
            close_direction_label: close_direction_label(
                app.state.app_settings.tab_list_position(),
            ),
            close_direction_icon: close_direction_icon(app.state.app_settings.tab_list_position()),
        }
    }
}

fn render_close_primary_button(ui: &mut egui::Ui) -> bool {
    primary_menu_button(ui, "tab_context.close_primary", "Close", X)
}

fn render_close_submenu(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    close_direction_label: &str,
    close_direction_icon: &str,
) {
    submenu_button(ui, "tab_context.close_caret", |ui| {
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
}

fn activate_slot(app: &mut ScratchpadApp, slot_index: usize) {
    if let Some(index) =
        crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(app, slot_index)
    {
        app.handle_command(AppCommand::Workspace(WorkspaceCommand::ActivateTab {
            index,
        }));
    } else if crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(app, slot_index)
    {
        app.handle_command(AppCommand::Settings(SettingsCommand::OpenSettings));
    }
}

fn tab_slot_path(app: &ScratchpadApp, slot_index: usize) -> Option<PathBuf> {
    if let Some(index) =
        crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(app, slot_index)
    {
        return app
            .tab_manager
            .tabs
            .as_slice()
            .get(index)
            .and_then(|tab| tab.active_buffer().path.clone());
    }

    crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(app, slot_index)
        .then(|| crate::app::app_state::settings_state::settings_path(app).to_path_buf())
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
