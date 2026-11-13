use crate::app::app_state::{PendingTabContextMenu, ScratchpadApp, workspace::display_tabs};
use crate::app::commands::{AppCommand, SettingsCommand, WorkspaceCommand};
use crate::app::ui::widget_ids;
use eframe::egui;
use egui_phosphor::regular::{FLOPPY_DISK, TABS, X, X_SQUARE};
use std::path::PathBuf;

mod close;
mod file_actions;
mod location_actions;
mod menu_ui;
mod order;
#[cfg(test)]
mod tests;

use self::menu_ui::{
    SUBMENU_WIDTH as TAB_CONTEXT_SUBMENU_WIDTH, WIDTH as TAB_CONTEXT_MENU_WIDTH,
    close_direction_icon, close_direction_label, menu_button, primary_menu_button, submenu_button,
};

struct TabContextMenuState {
    workspace_index: Option<usize>,
    is_settings: bool,
    path: Option<PathBuf>,
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
    let click = if secondary_clicked {
        TabContextClick::Secondary
    } else {
        TabContextClick::None
    };
    if secondary_clicked {
        display_tabs::select_only_tab_slot(app, slot_index);
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
    let menu_style =
        crate::app::ui::panel_style::menu_style(app.state.app_settings.editor_background_color());
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
        .style(menu_style)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .width(TAB_CONTEXT_MENU_WIDTH)
        .anchor(egui::pos2(pending.click_x, pending.click_y))
        .align(egui::RectAlign::BOTTOM_START)
        .align_alternatives(&[egui::RectAlign::BOTTOM_END])
        .open_bool(&mut pending.open)
        .show(|ui| render_tab_context_menu(ui, app, slot_index, &menu_state));

        app.state.dialogs.store_pending_tab_context_menu(pending);
        return click;
    }

    egui::Popup::context_menu(response)
        .style(menu_style)
        .show(|ui| render_tab_context_menu(ui, app, slot_index, &menu_state));
    click
}

pub(crate) fn attach_tab_list_context_menu(response: &egui::Response, app: &mut ScratchpadApp) {
    let _ = attach_tab_context_menu_impl(
        response,
        app,
        display_tabs::active_tab_slot_index(app),
        false,
    );
}

fn render_tab_context_menu(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    menu_state: &TabContextMenuState,
) {
    ui.set_min_width(TAB_CONTEXT_MENU_WIDTH);
    ui.set_max_width(TAB_CONTEXT_MENU_WIDTH);

    file_actions::render_file_actions(
        ui,
        app,
        slot_index,
        menu_state.workspace_index,
        menu_state.workspace_index.is_some(),
        menu_state.workspace_index.is_some(),
        menu_state.workspace_index.is_some(),
    );

    ui.separator();

    if let Some(auto_hide) = order::render_tab_list_actions(ui, app) {
        crate::app::app_state::settings_controller::set_auto_hide_tab_list(app, auto_hide);
    }
    order::render_tab_order_submenu(ui, app);

    ui.separator();

    location_actions::render_location_actions(
        ui,
        app,
        slot_index,
        menu_state.workspace_index.is_some(),
        menu_state.path.is_some(),
        menu_state.path.is_some(),
        menu_state.path.as_deref(),
    );

    ui.separator();

    if render_close_actions(ui, app, slot_index, menu_state.is_settings) {
        ui.close();
    }
}

/// "Close" with a caret submenu of bulk closes; returns whether "Close" was clicked.
fn render_close_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    is_settings: bool,
) -> bool {
    let clicked = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let clicked = primary_menu_button(ui, "tab_context.close_primary", "Close", X);
            render_close_submenu(ui, app, slot_index);
            clicked
        })
        .inner;
    if clicked {
        close::close_current_slot(app, slot_index, is_settings);
    }
    clicked
}

/// A bulk close run against the clicked slot.
type CloseSlots = fn(&mut ScratchpadApp, usize);

fn render_close_submenu(ui: &mut egui::Ui, app: &mut ScratchpadApp, slot_index: usize) {
    let position = app.state.app_settings.tab_list_position();
    let actions: [(&str, &str, CloseSlots); 4] = [
        ("Close others", TABS, close::close_other_slots),
        (
            close_direction_label(position),
            close_direction_icon(position),
            close::close_slots_after,
        ),
        ("Close saved", FLOPPY_DISK, |app, _| {
            close::close_saved_slots(app);
        }),
        ("Close all", X_SQUARE, |app, _| close::close_all_slots(app)),
    ];
    submenu_button(ui, "tab_context.close_caret", |ui| {
        for (label, icon, close) in actions {
            if menu_button(ui, TAB_CONTEXT_SUBMENU_WIDTH, label, Some(icon), true) {
                close(app, slot_index);
                ui.close();
            }
        }
    });
}

pub(super) fn activate_slot(app: &mut ScratchpadApp, slot_index: usize) {
    if let Some(index) = display_tabs::workspace_index_for_slot(app, slot_index) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ActivateTab { index }),
        );
    } else if display_tabs::tab_slot_is_settings(app, slot_index) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Settings(SettingsCommand::OpenSettings),
        );
    }
}

fn tab_slot_path(app: &ScratchpadApp, slot_index: usize) -> Option<PathBuf> {
    if let Some(index) = display_tabs::workspace_index_for_slot(app, slot_index) {
        return app
            .tab_manager
            .tabs
            .as_slice()
            .get(index)
            .and_then(|tab| tab.active_buffer().path.clone());
    }

    display_tabs::tab_slot_is_settings(app, slot_index)
        .then(|| crate::app::app_state::settings_state::settings_path(app).to_path_buf())
}

impl TabContextMenuState {
    fn new(app: &ScratchpadApp, slot_index: usize) -> Self {
        Self {
            workspace_index: display_tabs::workspace_index_for_slot(app, slot_index),
            is_settings: display_tabs::tab_slot_is_settings(app, slot_index),
            path: tab_slot_path(app, slot_index),
        }
    }
}
