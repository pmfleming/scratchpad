use super::layout::HeaderLayout;
use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::*;
use crate::app::commands::AppCommand;
use crate::app::theme::*;
use eframe::egui;

pub(crate) fn show_primary_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let button_spacing = 4.0;
    let width = BUTTON_SIZE.x * 3.0 + button_spacing * 2.0;

    ui.allocate_ui_with_layout(
        egui::vec2(width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            if phosphor_button(
                ui,
                egui_phosphor::regular::FOLDER_OPEN,
                BUTTON_SIZE,
                action_bg(ui),
                action_hover_bg(ui),
                "Open File",
            )
            .clicked()
            {
                app.handle_command(AppCommand::OpenFile);
            }

            ui.add_space(button_spacing);
            if phosphor_button(
                ui,
                egui_phosphor::regular::FLOPPY_DISK,
                BUTTON_SIZE,
                action_bg(ui),
                action_hover_bg(ui),
                "Save As",
            )
            .clicked()
            {
                app.handle_command(AppCommand::SaveFileAs);
            }

            ui.add_space(button_spacing);
            if phosphor_button(
                ui,
                egui_phosphor::regular::MAGNIFYING_GLASS,
                BUTTON_SIZE,
                action_bg(ui),
                action_hover_bg(ui),
                "Search",
            )
            .clicked()
            {
                app.set_warning_status("Search is not implemented yet.");
            }
        },
    );
}

pub(crate) fn show_vertical_primary_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let button_spacing = 4.0;
    let button_size = CAPTION_BUTTON_SIZE;
    let available_width = ui
        .available_width()
        .max(button_size.x * 2.0 + button_spacing);
    let maximized = ui.input(|input| input.viewport().maximized.unwrap_or(false));
    let left_buttons = [
        VerticalActionButton::new(
            egui_phosphor::regular::FOLDER_OPEN,
            "Open File",
            VerticalAction::OpenFile,
        ),
        VerticalActionButton::new(
            egui_phosphor::regular::FLOPPY_DISK,
            "Save",
            VerticalAction::SaveFile,
        ),
        VerticalActionButton::new(
            egui_phosphor::regular::MAGNIFYING_GLASS,
            "Search",
            VerticalAction::Search,
        ),
    ];
    let right_buttons = [
        VerticalActionButton::new(
            egui_phosphor::regular::MINUS,
            "Minimize",
            VerticalAction::Minimize,
        ),
        VerticalActionButton::new(
            if maximized {
                egui_phosphor::regular::COPY
            } else {
                egui_phosphor::regular::SQUARE
            },
            if maximized { "Restore" } else { "Maximize" },
            VerticalAction::ToggleMaximize,
        ),
        VerticalActionButton::new(
            egui_phosphor::regular::X,
            "Close",
            VerticalAction::CloseWindow,
        ),
    ];

    let six_button_width = button_size.x * 6.0 + button_spacing * 5.0;
    let three_button_width = button_size.x * 3.0 + button_spacing * 2.0;

    if available_width >= six_button_width {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = button_spacing;
            render_button_group(ui, app, &left_buttons, button_size);
            ui.add_space((ui.available_width() - three_button_width).max(button_spacing));
            render_button_group(ui, app, &right_buttons, button_size);
        });
    } else if available_width >= three_button_width {
        render_button_row(ui, app, &left_buttons, button_size, button_spacing);
        ui.add_space(button_spacing);
        render_button_row(ui, app, &right_buttons, button_size, button_spacing);
    } else {
        let buttons = [
            left_buttons[0],
            left_buttons[1],
            left_buttons[2],
            right_buttons[0],
            right_buttons[1],
            right_buttons[2],
        ];
        for row in buttons.chunks(2) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = button_spacing;
                render_button(ui, app, row[0], button_size);
                if let Some(button) = row.get(1) {
                    ui.add_space((ui.available_width() - button_size.x).max(button_spacing));
                    render_button(ui, app, *button, button_size);
                }
            });
            ui.add_space(button_spacing);
        }
    }
}

#[derive(Clone, Copy)]
struct VerticalActionButton {
    icon: &'static str,
    tooltip: &'static str,
    action: VerticalAction,
}

impl VerticalActionButton {
    fn new(icon: &'static str, tooltip: &'static str, action: VerticalAction) -> Self {
        Self {
            icon,
            tooltip,
            action,
        }
    }
}

#[derive(Clone, Copy)]
enum VerticalAction {
    OpenFile,
    SaveFile,
    Search,
    Minimize,
    ToggleMaximize,
    CloseWindow,
}

fn handle_vertical_action(ctx: &egui::Context, app: &mut ScratchpadApp, action: VerticalAction) {
    match action {
        VerticalAction::OpenFile => app.handle_command(AppCommand::OpenFile),
        VerticalAction::SaveFile => app.handle_command(AppCommand::SaveFile),
        VerticalAction::Search => app.set_warning_status("Search is not implemented yet."),
        VerticalAction::Minimize => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        VerticalAction::ToggleMaximize => {
            let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        VerticalAction::CloseWindow => app.request_exit(ctx),
    }
}

fn render_button_row(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    buttons: &[VerticalActionButton],
    button_size: egui::Vec2,
    button_spacing: f32,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = button_spacing;
        render_button_group(ui, app, buttons, button_size);
    });
}

fn render_button_group(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    buttons: &[VerticalActionButton],
    button_size: egui::Vec2,
) {
    for button in buttons {
        render_button(ui, app, *button, button_size);
    }
}

fn render_button(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    button: VerticalActionButton,
    button_size: egui::Vec2,
) {
    let (background, hover_background) = if matches!(button.action, VerticalAction::CloseWindow) {
        (CLOSE_BG, CLOSE_HOVER_BG)
    } else {
        (action_bg(ui), action_hover_bg(ui))
    };
    if phosphor_button(
        ui,
        button.icon,
        button_size,
        background,
        hover_background,
        button.tooltip,
    )
    .clicked()
    {
        handle_vertical_action(ui.ctx(), app, button.action);
    }
}

pub(crate) fn show_caption_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
) {
    if caption_controls(ui, ctx, layout.caption_controls_width) {
        app.request_exit(ctx);
    }
}
