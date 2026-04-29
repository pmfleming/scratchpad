use super::TileRenderRequest;
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::SplitAxis;
use crate::app::theme::*;
use crate::app::ui::tile_header::TileAction;
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_CLOCKWISE, ARROW_COUNTER_CLOCKWISE, ARROW_DOWN, ARROW_LEFT, ARROW_LINE_UP, ARROW_RIGHT,
    ARROW_UP, ARROWS_COUNTER_CLOCKWISE, ARROWS_SPLIT, CARET_RIGHT, CLIPBOARD_TEXT,
    CLOCK_COUNTER_CLOCKWISE, COPY, FLOPPY_DISK, FOLDER_OPEN, MAGNIFYING_GLASS, SCISSORS,
    SELECTION_ALL, X,
};

const DEFAULT_SPLIT_RATIO: f32 = 0.5;
const EDITOR_CONTEXT_MENU_WIDTH: f32 = 192.0;
const EDITOR_CONTEXT_SUBMENU_WIDTH: f32 = 168.0;
const EDITOR_CONTEXT_ICON_BUTTON_SIZE: egui::Vec2 = egui::vec2(38.0, 30.0);
const EDITOR_CONTEXT_CARET_WIDTH: f32 = 28.0;

pub(super) fn attach_editor_context_menu(
    tile_response: &egui::Response,
    _ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
    actions: &mut Vec<TileAction>,
) {
    activate_inactive_tile_on_secondary_click(app, tile_response, request);

    let can_promote = app.tabs()[request.tab_index].can_promote_view(request.view_id);
    let save_existing = app.tabs()[request.tab_index]
        .buffer_for_view(request.view_id)
        .is_some_and(|buffer| buffer.path.is_some());
    tile_response.context_menu(|ui| {
        set_menu_width(ui, EDITOR_CONTEXT_MENU_WIDTH);
        render_history_menu(ui, app);
        ui.separator();
        render_file_menu(ui, app, save_existing);
        ui.separator();
        render_tile_menu(ui, actions, request, can_promote);
        ui.separator();
        render_icon_rail_menu(ui, app);
    });
}

pub(super) fn activate_inactive_tile_on_secondary_click(
    app: &mut ScratchpadApp,
    tile_response: &egui::Response,
    request: TileRenderRequest,
) {
    if tile_response.secondary_clicked() && !request.is_active {
        app.activate_view(request.view_id);
        app.request_focus_for_view(request.view_id);
    }
}

fn set_menu_width(ui: &mut egui::Ui, width: f32) {
    ui.set_min_width(width);
    ui.set_max_width(width);
}

fn render_history_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    run_menu_command(
        ui,
        app,
        "Undo",
        Some(ARROW_COUNTER_CLOCKWISE),
        app.active_buffer_can_undo_text_operation(),
        AppCommand::UndoActiveBufferTextOperation,
        true,
    );
    run_menu_command(
        ui,
        app,
        "Redo",
        Some(ARROW_CLOCKWISE),
        app.active_buffer_can_redo_text_operation(),
        AppCommand::RedoActiveBufferTextOperation,
        true,
    );
    run_menu_command(
        ui,
        app,
        "History",
        Some(CLOCK_COUNTER_CLOCKWISE),
        true,
        AppCommand::OpenHistory,
        false,
    );
}

fn render_file_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp, save_existing: bool) {
    run_menu_command(
        ui,
        app,
        "Find",
        Some(MAGNIFYING_GLASS),
        true,
        AppCommand::OpenSearch,
        false,
    );
    run_menu_command(
        ui,
        app,
        "Replace",
        Some(ARROWS_COUNTER_CLOCKWISE),
        true,
        AppCommand::OpenSearchAndReplace,
        false,
    );
    run_menu_command(
        ui,
        app,
        "Open File Here",
        Some(FOLDER_OPEN),
        true,
        AppCommand::OpenFileHere,
        true,
    );
    run_save_menu_action(ui, app, save_existing);
}

fn render_tile_menu(
    ui: &mut egui::Ui,
    actions: &mut Vec<TileAction>,
    request: TileRenderRequest,
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

fn render_icon_rail_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let any_action = ui
        .with_layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.horizontal(|ui| {
                    run_icon_rail_action(ui, app, SCISSORS, "Cut", |ui, app| {
                        copy_icon_text(ui, app.cut_selected_text_in_active_view())
                    }) || run_icon_rail_action(ui, app, COPY, "Copy", |ui, app| {
                        copy_icon_text(ui, app.copy_selected_text_in_active_view())
                    }) || run_icon_rail_action(ui, app, CLIPBOARD_TEXT, "Paste", |ui, _| {
                        ui.ctx()
                            .clone()
                            .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                        true
                    }) || run_icon_rail_action(ui, app, SELECTION_ALL, "Select All", |_, app| {
                        app.select_all_in_active_view()
                    })
                })
                .inner
            },
        )
        .inner;

    if any_action {
        app.request_focus_for_active_view();
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
            app.handle_command(command);
            if request_focus {
                app.request_focus_for_active_view();
            }
            true
        },
        app,
    )
}

fn icon_action_clicked(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> bool {
    icon_rail_button(ui, icon, tooltip, true).clicked()
}

fn run_save_menu_action(ui: &mut egui::Ui, app: &mut ScratchpadApp, save_existing: bool) -> bool {
    run_context_menu_action(
        ui,
        if save_existing { "Save" } else { "Save As" },
        Some(FLOPPY_DISK),
        true,
        |_, app| {
            app.request_focus_for_active_view();
            if save_existing {
                app.save_file();
            } else {
                app.save_file_as();
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

fn run_icon_rail_action(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    icon: &str,
    tooltip: &str,
    action: impl FnOnce(&mut egui::Ui, &mut ScratchpadApp) -> bool,
) -> bool {
    icon_action_clicked(ui, icon, tooltip) && action(ui, app)
}

fn copy_icon_text(ui: &mut egui::Ui, text: Option<String>) -> bool {
    text.is_some_and(|text| {
        ui.copy_text(text);
        true
    })
}

fn menu_action_button(ui: &mut egui::Ui, label: &str, icon: Option<&str>, enabled: bool) -> bool {
    let text = match icon {
        Some(icon) => format!("{icon}  {label}"),
        None => label.to_owned(),
    };
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(text).color(text_primary(ui)))
                .min_size(egui::vec2(EDITOR_CONTEXT_MENU_WIDTH, 28.0))
                .stroke(egui::Stroke::NONE),
        )
        .clicked()
    })
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

fn split_menu_button(ui: &mut egui::Ui, label: &str, icon: &str) -> bool {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        ui.add(
            egui::Button::new(
                egui::RichText::new(format!("{icon}  {label}")).color(text_primary(ui)),
            )
            .min_size(egui::vec2(EDITOR_CONTEXT_SUBMENU_WIDTH, 28.0))
            .stroke(egui::Stroke::NONE),
        )
        .clicked()
    })
}

fn render_split_primary_button(ui: &mut egui::Ui) -> bool {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let response = ui.add(
            egui::Button::new("")
                .min_size(egui::vec2(
                    EDITOR_CONTEXT_MENU_WIDTH - EDITOR_CONTEXT_CARET_WIDTH,
                    28.0,
                ))
                .stroke(egui::Stroke::NONE),
        );
        ui.painter().text(
            response.rect.left_center() + egui::vec2(10.0, 0.0),
            egui::Align2::LEFT_CENTER,
            format!("{ARROWS_SPLIT}  Split"),
            egui::TextStyle::Button.resolve(ui.style()),
            text_primary(ui),
        );
        response.clicked()
    })
}

fn render_split_submenu(ui: &mut egui::Ui, actions: &mut Vec<TileAction>) {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let button = egui::Button::new(egui::RichText::new(CARET_RIGHT).color(text_primary(ui)))
            .min_size(egui::vec2(EDITOR_CONTEXT_CARET_WIDTH, 28.0))
            .stroke(egui::Stroke::NONE);

        egui::containers::menu::SubMenuButton::from_button(button).ui(ui, |ui| {
            set_menu_width(ui, EDITOR_CONTEXT_SUBMENU_WIDTH);

            for (label, icon, direction) in [
                ("Split Left", ARROW_LEFT, SplitDirection::Left),
                ("Split Right", ARROW_RIGHT, SplitDirection::Right),
                ("Split Up", ARROW_UP, SplitDirection::Up),
                ("Split Down", ARROW_DOWN, SplitDirection::Down),
            ] {
                if split_menu_button(ui, label, icon) {
                    queue_split_action(actions, direction);
                    ui.close();
                }
            }
        });
    });
}

#[derive(Clone, Copy)]
enum SplitDirection {
    Left,
    Right,
    Up,
    Down,
}

fn queue_split_action(actions: &mut Vec<TileAction>, direction: SplitDirection) {
    let (axis, new_view_first) = match direction {
        SplitDirection::Left => (SplitAxis::Vertical, true),
        SplitDirection::Right => (SplitAxis::Vertical, false),
        SplitDirection::Up => (SplitAxis::Horizontal, true),
        SplitDirection::Down => (SplitAxis::Horizontal, false),
    };
    actions.push(TileAction::Split {
        axis,
        new_view_first,
        ratio: DEFAULT_SPLIT_RATIO,
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

fn icon_rail_button(ui: &mut egui::Ui, icon: &str, tooltip: &str, enabled: bool) -> egui::Response {
    with_visual_overrides(ui, apply_icon_rail_button_style, |ui| {
        let button = egui::Button::new(
            egui::RichText::new(icon)
                .font(egui::FontId::proportional(17.0))
                .color(text_primary(ui)),
        )
        .min_size(EDITOR_CONTEXT_ICON_BUTTON_SIZE)
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(8));

        ui.add_enabled(enabled, button).on_hover_text(tooltip)
    })
}

fn apply_icon_rail_button_style(ui: &mut egui::Ui) {
    let idle_bg = action_bg(ui);
    let hover_bg = action_hover_bg(ui);
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.bg_fill = idle_bg;
    visuals.widgets.inactive.weak_bg_fill = idle_bg;
    visuals.widgets.hovered.bg_fill = hover_bg;
    visuals.widgets.hovered.weak_bg_fill = hover_bg;
    visuals.widgets.active.bg_fill = hover_bg;
    visuals.widgets.active.weak_bg_fill = hover_bg;
    visuals.widgets.open.bg_fill = hover_bg;
    visuals.widgets.open.weak_bg_fill = hover_bg;
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
