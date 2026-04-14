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
    let available_width = ui.available_width().max(button_size.x);
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

    match vertical_primary_actions_layout(available_width, button_size.x, button_spacing) {
        VerticalPrimaryActionsLayout::SingleRow => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = button_spacing;
                render_button_group(ui, app, &left_buttons, button_size);
                let caption_width = button_size.x * right_buttons.len() as f32
                    + button_spacing * right_buttons.len().saturating_sub(1) as f32;
                ui.add_space((ui.available_width() - caption_width).max(button_spacing));
                render_button_group(ui, app, &right_buttons, button_size);
            });
        }
        VerticalPrimaryActionsLayout::CaptionFirstRows { buttons_per_row } => {
            render_wrapped_button_section(
                ui,
                app,
                &right_buttons,
                button_size,
                button_spacing,
                buttons_per_row,
                true,
            );
            ui.add_space(button_spacing);
            render_wrapped_button_section(
                ui,
                app,
                &left_buttons,
                button_size,
                button_spacing,
                buttons_per_row,
                false,
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerticalPrimaryActionsLayout {
    SingleRow,
    CaptionFirstRows { buttons_per_row: usize },
}

fn vertical_primary_actions_layout(
    available_width: f32,
    button_width: f32,
    button_spacing: f32,
) -> VerticalPrimaryActionsLayout {
    let six_button_width = button_width * 6.0 + button_spacing * 5.0;
    let three_button_width = button_width * 3.0 + button_spacing * 2.0;
    let two_button_width = button_width * 2.0 + button_spacing;

    if available_width >= six_button_width {
        VerticalPrimaryActionsLayout::SingleRow
    } else if available_width >= three_button_width {
        VerticalPrimaryActionsLayout::CaptionFirstRows { buttons_per_row: 3 }
    } else if available_width >= two_button_width {
        VerticalPrimaryActionsLayout::CaptionFirstRows { buttons_per_row: 2 }
    } else {
        VerticalPrimaryActionsLayout::CaptionFirstRows { buttons_per_row: 1 }
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

fn render_wrapped_button_section(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    buttons: &[VerticalActionButton],
    button_size: egui::Vec2,
    button_spacing: f32,
    buttons_per_row: usize,
    right_justified: bool,
) {
    let row_count = buttons.len().div_ceil(buttons_per_row);
    for (row_index, row) in buttons.chunks(buttons_per_row).enumerate() {
        render_aligned_button_row(ui, app, row, button_size, button_spacing, right_justified);
        if row_index + 1 < row_count {
            ui.add_space(button_spacing);
        }
    }
}

fn render_aligned_button_row(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    buttons: &[VerticalActionButton],
    button_size: egui::Vec2,
    button_spacing: f32,
    right_justified: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = button_spacing;
        if right_justified {
            ui.add_space(right_justified_row_leading_space(
                ui.available_width(),
                buttons.len(),
                button_size.x,
                button_spacing,
            ));
        }
        render_button_group(ui, app, buttons, button_size);
    });
}

fn right_justified_row_leading_space(
    available_width: f32,
    button_count: usize,
    button_width: f32,
    button_spacing: f32,
) -> f32 {
    (available_width - row_width(button_count, button_width, button_spacing)).max(0.0)
}

fn row_width(button_count: usize, button_width: f32, button_spacing: f32) -> f32 {
    button_width * button_count as f32 + button_spacing * button_count.saturating_sub(1) as f32
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

#[cfg(test)]
mod tests {
    use super::{
        VerticalPrimaryActionsLayout, right_justified_row_leading_space,
        vertical_primary_actions_layout,
    };

    #[test]
    fn wide_vertical_actions_stay_on_one_row() {
        assert_eq!(
            vertical_primary_actions_layout(236.0, 36.0, 4.0),
            VerticalPrimaryActionsLayout::SingleRow
        );
    }

    #[test]
    fn medium_vertical_actions_stack_with_caption_controls_first() {
        assert_eq!(
            vertical_primary_actions_layout(116.0, 36.0, 4.0),
            VerticalPrimaryActionsLayout::CaptionFirstRows { buttons_per_row: 3 }
        );
    }

    #[test]
    fn narrow_vertical_actions_keep_caption_controls_above_primary_actions() {
        assert_eq!(
            vertical_primary_actions_layout(96.0, 36.0, 4.0),
            VerticalPrimaryActionsLayout::CaptionFirstRows { buttons_per_row: 2 }
        );
    }

    #[test]
    fn caption_rows_use_remaining_width_as_leading_space() {
        assert_eq!(right_justified_row_leading_space(96.0, 2, 36.0, 4.0), 20.0);
        assert_eq!(right_justified_row_leading_space(96.0, 1, 36.0, 4.0), 60.0);
    }
}
