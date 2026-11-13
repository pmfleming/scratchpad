mod model;
mod unicode_menu;
mod widgets;
mod zoom;

use super::TileRenderRequest;
use crate::app::app_state::{
    ScratchpadApp,
    workspace::{accessors as workspace_accessors, editing as workspace_editing},
};
use crate::app::commands::{
    AppCommand, DialogCommand, EditCommand, SearchCommand, WorkspaceCommand,
};
use crate::app::shortcut_tooltips;
use crate::app::ui::{panel_style, tile_header::TileAction, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_CLOCKWISE, ARROW_COUNTER_CLOCKWISE, ARROW_LINE_UP, ARROWS_COUNTER_CLOCKWISE,
    ARROWS_SPLIT, CLIPBOARD_TEXT, CLOCK_COUNTER_CLOCKWISE, COPY, MAGNIFYING_GLASS, SCISSORS,
    SELECTION_ALL, TRASH, X,
};
use model::{
    SPLIT_MENU_ITEMS, queue_split_action, shortcut_tooltip_for_menu_label,
    should_activate_tile_on_secondary_click,
};
use unicode_menu::render_display_unicode_menu;
use widgets::{
    EDITOR_CONTEXT_MENU_WIDTH, EDITOR_CONTEXT_ROW_HEIGHT, icon_rail_button,
    icon_rail_leading_space, menu_action_button, set_menu_width,
};

pub(super) fn attach_editor_context_menu(
    tile_response: &egui::Response,
    _ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: &TileRenderRequest,
    actions: &mut Vec<TileAction>,
) {
    activate_inactive_tile_on_secondary_click(app, tile_response, request);

    let can_promote = crate::app::domain::tab::summary::can_promote_view(
        &app.tab_manager.tabs.as_slice()[request.tab_index],
        request.view_id,
    );
    egui::Popup::context_menu(tile_response)
        .style(panel_style::menu_style(
            app.state.app_settings.editor_background_color(),
        ))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            set_menu_width(ui, EDITOR_CONTEXT_MENU_WIDTH);
            render_standard_edit_menu(ui, app);
            ui.separator();
            render_history_menu(ui, app);
            ui.separator();
            render_display_unicode_menu(ui, app);
            zoom::render(ui, app, request.view_id);
            ui.separator();
            render_file_menu(ui, app);
            ui.separator();
            render_tile_menu(ui, actions, request, can_promote);
            ui.separator();
            render_edit_button_rail(ui, app);
        });
}

pub(super) fn activate_inactive_tile_on_secondary_click(
    app: &mut ScratchpadApp,
    tile_response: &egui::Response,
    request: &TileRenderRequest,
) {
    if should_activate_tile_on_secondary_click(tile_response.secondary_clicked(), request.is_active)
    {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ActivateView {
                view_id: request.view_id,
            }),
        );
        workspace_accessors::request_focus_for_view(app, request.view_id);
    }
}

fn render_standard_edit_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    run_menu_command(
        ui,
        app,
        "Undo",
        Some(ARROW_COUNTER_CLOCKWISE),
        workspace_editing::active_buffer_can_undo_text_operation(app),
        AppCommand::Edit(EditCommand::UndoActiveBufferTextOperation),
        true,
    );
    run_menu_command(
        ui,
        app,
        "Redo",
        Some(ARROW_CLOCKWISE),
        workspace_editing::active_buffer_can_redo_text_operation(app),
        AppCommand::Edit(EditCommand::RedoActiveBufferTextOperation),
        true,
    );
    run_context_menu_action(
        ui,
        "Delete",
        Some(TRASH),
        workspace_editing::copy_selected_text_in_active_view(app).is_some(),
        |_, app| workspace_editing::delete_selected_text_in_active_view(app),
        app,
    );
}

fn render_history_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    run_menu_command(
        ui,
        app,
        "History",
        Some(CLOCK_COUNTER_CLOCKWISE),
        true,
        AppCommand::Dialog(DialogCommand::OpenTextHistory),
        false,
    );
}

fn render_file_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    run_menu_command(
        ui,
        app,
        "Find",
        Some(MAGNIFYING_GLASS),
        true,
        AppCommand::Search(SearchCommand::Open),
        false,
    );
    run_menu_command(
        ui,
        app,
        "Replace",
        Some(ARROWS_COUNTER_CLOCKWISE),
        true,
        AppCommand::Search(SearchCommand::OpenAndReplace),
        false,
    );
}

fn render_tile_menu(
    ui: &mut egui::Ui,
    actions: &mut Vec<TileAction>,
    request: &TileRenderRequest,
    can_promote: bool,
) {
    split_menu_row(ui, actions);
    if menu_action_button(ui, "Promote to tab", Some(ARROW_LINE_UP), can_promote) {
        actions.push(TileAction::Promote(request.view_id));
        ui.close();
    }
    if menu_action_button(ui, "Close pane", Some(X), request.multiple_tiles_visible) {
        actions.push(TileAction::Close(request.view_id));
        ui.close();
    }
}

fn render_edit_button_rail(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let selection_available = workspace_editing::copy_selected_text_in_active_view(app).is_some();
    let any_action = ui
        .horizontal(|ui| {
            let button_spacing = ui.spacing().item_spacing.x;
            ui.add_space(icon_rail_leading_space(
                ui.available_width(),
                button_spacing,
            ));

            run_icon_rail_action(
                ui,
                app,
                SCISSORS,
                shortcut_tooltips::CUT,
                selection_available,
                |ui, app| {
                    copy_icon_text(ui, workspace_editing::cut_selected_text_in_active_view(app))
                },
            ) || run_icon_rail_action(
                ui,
                app,
                COPY,
                shortcut_tooltips::COPY,
                selection_available,
                |ui, app| {
                    copy_icon_text(
                        ui,
                        workspace_editing::copy_selected_text_in_active_view(app),
                    )
                },
            ) || run_icon_rail_action(
                ui,
                app,
                CLIPBOARD_TEXT,
                shortcut_tooltips::PASTE,
                true,
                |ui, _| {
                    ui.ctx()
                        .clone()
                        .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                    true
                },
            ) || run_icon_rail_action(
                ui,
                app,
                SELECTION_ALL,
                shortcut_tooltips::SELECT_ALL,
                true,
                |_, app| workspace_editing::select_all_in_active_view(app),
            )
        })
        .inner;

    if any_action {
        workspace_accessors::request_focus_for_active_view(app);
        ui.close();
    }
}

fn run_menu_command(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    label: &str,
    icon: Option<&str>,
    enabled: bool,
    command: AppCommand,
    request_focus: bool,
) -> bool {
    run_context_menu_action(
        ui,
        label,
        icon,
        enabled,
        |_, app| {
            crate::app::commands::handle_command(app, command);
            if request_focus {
                workspace_accessors::request_focus_for_active_view(app);
            }
            true
        },
        app,
    )
}

fn run_context_menu_action(
    ui: &mut egui::Ui,
    label: &str,
    icon: Option<&str>,
    enabled: bool,
    action: impl FnOnce(&mut egui::Ui, &mut ScratchpadApp) -> bool,
    app: &mut ScratchpadApp,
) -> bool {
    if !menu_action_button(ui, label, icon, enabled) {
        return false;
    }

    let handled = action(ui, app);
    if handled {
        ui.close();
    }
    handled
}

fn copy_icon_text(ui: &mut egui::Ui, text: Option<String>) -> bool {
    text.is_some_and(|text| {
        ui.copy_text(text);
        true
    })
}

fn run_icon_rail_action(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    icon: &str,
    tooltip: &str,
    enabled: bool,
    action: impl FnOnce(&mut egui::Ui, &mut ScratchpadApp) -> bool,
) -> bool {
    icon_rail_button(ui, icon, tooltip, enabled).clicked() && action(ui, app)
}

/// "Split" with the four directions inline, one click each.
fn split_menu_row(ui: &mut egui::Ui, actions: &mut Vec<TileAction>) {
    let rect = widget_ids::allocate_exact_rect(
        ui,
        egui::vec2(EDITOR_CONTEXT_MENU_WIDTH, EDITOR_CONTEXT_ROW_HEIGHT),
    );
    panel_style::paint_menu_row(ui, rect, Some(ARROWS_SPLIT), "Split", None, true);
    widget_ids::rect_scope_with_layout(
        ui,
        rect.shrink2(egui::vec2(6.0, 1.0)),
        "editor_context.split",
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for item in SPLIT_MENU_ITEMS.iter().rev() {
                let hint = shortcut_tooltip_for_menu_label(ui.ctx(), item.label)
                    .unwrap_or_else(|| item.label.to_owned());
                if panel_style::icon_button_with_hint(
                    ui,
                    ("editor_context.split", item.label),
                    item.icon,
                    item.label,
                    &hint,
                    true,
                )
                .clicked()
                {
                    queue_split_action(actions, item.direction);
                    ui.close();
                }
            }
        },
    );
}
