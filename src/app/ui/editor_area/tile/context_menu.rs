mod model;
mod unicode_menu;
mod widgets;

use super::TileRenderRequest;
use crate::app::app_state::{
    ScratchpadApp,
    workspace::{accessors as workspace_accessors, editing as workspace_editing},
};
use crate::app::commands::{
    AppCommand, DialogCommand, EditCommand, SearchCommand, WorkspaceCommand,
};
use crate::app::shortcut_keymap::ShortcutAction;
use crate::app::shortcut_tooltips;
use crate::app::theme::text_primary;
use crate::app::ui::tile_header::TileAction;
use crate::app::ui::widget_ids;
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_CLOCKWISE, ARROW_COUNTER_CLOCKWISE, ARROW_LINE_UP, ARROWS_COUNTER_CLOCKWISE,
    ARROWS_SPLIT, CARET_RIGHT, CLIPBOARD_TEXT, CLOCK_COUNTER_CLOCKWISE, COPY, MAGNIFYING_GLASS,
    SCISSORS, SELECTION_ALL, TRASH, X,
};
use model::{
    SPLIT_MENU_ITEMS, SplitDirection, queue_split_action, should_activate_tile_on_secondary_click,
};
use unicode_menu::render_display_unicode_menu;
use widgets::{
    EDITOR_CONTEXT_CARET_WIDTH, EDITOR_CONTEXT_MENU_WIDTH, EDITOR_CONTEXT_ROW_HEIGHT,
    EDITOR_CONTEXT_SUBMENU_WIDTH, apply_context_menu_row_hover_style, icon_rail_button,
    icon_rail_leading_space, menu_action_button, paint_context_menu_row_label, set_menu_width,
    split_menu_button, with_visual_overrides,
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
    tile_response.context_menu(|ui| {
        set_menu_width(ui, EDITOR_CONTEXT_MENU_WIDTH);
        render_standard_edit_menu(ui, app);
        ui.separator();
        render_history_menu(ui, app);
        ui.separator();
        render_display_unicode_menu(ui, app);
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
    if menu_action_button(ui, "Promote Tile", Some(ARROW_LINE_UP), can_promote) {
        actions.push(TileAction::Promote(request.view_id));
        ui.close();
    }
    if menu_action_button(ui, "Close Tile", Some(X), request.can_close) {
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

fn split_menu_row(ui: &mut egui::Ui, actions: &mut Vec<TileAction>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        let split_clicked = render_split_primary_button(ui);
        render_split_submenu(ui, actions);

        if split_clicked {
            queue_split_action(actions, SplitDirection::Right);
        }
    });
}

fn render_split_primary_button(ui: &mut egui::Ui) -> bool {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let response = widget_ids::surface_response(
            ui,
            "editor_context.split_primary",
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add(
                    egui::Button::new("")
                        .min_size(egui::vec2(
                            EDITOR_CONTEXT_MENU_WIDTH - EDITOR_CONTEXT_CARET_WIDTH,
                            28.0,
                        ))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        paint_context_menu_row_label(ui, response.rect, Some(ARROWS_SPLIT), "Split", true);
        let clicked = response.clicked();
        response.on_hover_text(shortcut_tooltips::action(
            ui.ctx(),
            ShortcutAction::SplitRight,
            "Split Right",
        ));
        clicked
    })
}

fn render_split_submenu(ui: &mut egui::Ui, actions: &mut Vec<TileAction>) {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let button = egui::Button::new(egui::RichText::new(CARET_RIGHT).color(text_primary(ui)))
            .min_size(egui::vec2(
                EDITOR_CONTEXT_CARET_WIDTH,
                EDITOR_CONTEXT_ROW_HEIGHT,
            ))
            .stroke(egui::Stroke::NONE);

        widget_ids::surface_widget(ui, "editor_context.split_caret", "submenu", |ui| {
            egui::containers::menu::SubMenuButton::from_button(button).ui(ui, |ui| {
                set_menu_width(ui, EDITOR_CONTEXT_SUBMENU_WIDTH);
                render_split_submenu_items(ui, actions);
            });
        });
    });
}

fn render_split_submenu_items(ui: &mut egui::Ui, actions: &mut Vec<TileAction>) {
    for item in SPLIT_MENU_ITEMS {
        if split_menu_button(ui, item.label, item.icon) {
            queue_split_action(actions, item.direction);
            ui.close();
        }
    }
}
