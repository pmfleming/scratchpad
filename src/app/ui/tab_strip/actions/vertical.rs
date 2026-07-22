use crate::app::app_state::{ScratchpadApp, frame};
use crate::app::chrome::{
    PhosphorButtonColors, phosphor_button, phosphor_button_with_hover_icon_color,
};
use crate::app::commands::{AppCommand, FileCommand, SearchCommand};
use crate::app::shortcut_keymap::ShortcutAction;
use crate::app::shortcut_tooltips;
use crate::app::theme::{CAPTION_BUTTON_SIZE, CLOSE_HOVER_BG, action_bg, action_hover_bg};
use eframe::egui;
use std::borrow::Cow;

const BUTTON_SPACING: f32 = 4.0;

pub(super) fn show_vertical_primary_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    show_file_buttons: bool,
    show_caption_buttons: bool,
) -> bool {
    let file_buttons = show_file_buttons.then(|| file_action_buttons(ui.ctx()));
    let caption_buttons = show_caption_buttons.then(|| caption_action_buttons(ui));
    let left = file_buttons
        .as_ref()
        .map_or(&[][..], |buttons| &buttons[..]);
    let right = caption_buttons
        .as_ref()
        .map_or(&[][..], |buttons| &buttons[..]);
    if left.is_empty() && right.is_empty() {
        return false;
    }

    render_vertical_actions(ui, app, left, right);
    true
}

fn file_action_buttons(ctx: &egui::Context) -> [VerticalActionButton; 3] {
    [
        VerticalActionButton::new(
            egui_phosphor::regular::FOLDER_OPEN,
            shortcut_tooltips::action(ctx, ShortcutAction::OpenFile, "Open File"),
            VerticalAction::OpenFile,
        ),
        VerticalActionButton::new(
            egui_phosphor::regular::FLOPPY_DISK,
            shortcut_tooltips::action(ctx, ShortcutAction::SaveFile, "Save"),
            VerticalAction::SaveFile,
        ),
        VerticalActionButton::new(
            egui_phosphor::regular::MAGNIFYING_GLASS,
            shortcut_tooltips::action(ctx, ShortcutAction::OpenSearch, "Search"),
            VerticalAction::Search,
        ),
    ]
}

fn caption_action_buttons(ui: &egui::Ui) -> [VerticalActionButton; 3] {
    let maximized = ui.input(|input| input.viewport().maximized.unwrap_or(false));
    let (maximize_icon, maximize_tooltip) = if maximized {
        (egui_phosphor::regular::COPY, "Restore")
    } else {
        (egui_phosphor::regular::SQUARE, "Maximize")
    };
    [
        VerticalActionButton::new(
            egui_phosphor::regular::MINUS,
            "Minimize",
            VerticalAction::Minimize,
        ),
        VerticalActionButton::new(
            maximize_icon,
            maximize_tooltip,
            VerticalAction::ToggleMaximize,
        ),
        VerticalActionButton::new(
            egui_phosphor::regular::X,
            "Close",
            VerticalAction::CloseWindow,
        ),
    ]
}

fn render_vertical_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    left: &[VerticalActionButton],
    right: &[VerticalActionButton],
) {
    let button_size = CAPTION_BUTTON_SIZE;
    let layout = vertical_primary_actions_layout(
        ui.available_width().max(button_size.x),
        button_size.x,
        BUTTON_SPACING,
        left.len(),
        right.len(),
    );
    match layout {
        VerticalPrimaryActionsLayout::SingleRow => {
            render_single_button_row(ui, app, left, right, button_size);
        }
        VerticalPrimaryActionsLayout::CaptionFirstRows { buttons_per_row } => {
            render_caption_first_rows(ui, app, left, right, button_size, buttons_per_row);
        }
    }
}

fn render_single_button_row(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    left: &[VerticalActionButton],
    right: &[VerticalActionButton],
    button_size: egui::Vec2,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_SPACING;
        render_button_group(ui, app, left, button_size);
        if !right.is_empty() {
            let caption_width = row_width(right.len(), button_size.x, BUTTON_SPACING);
            ui.add_space((ui.available_width() - caption_width).max(BUTTON_SPACING));
            render_button_group(ui, app, right, button_size);
        }
    });
}

fn render_caption_first_rows(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    left: &[VerticalActionButton],
    right: &[VerticalActionButton],
    button_size: egui::Vec2,
    buttons_per_row: usize,
) {
    if !right.is_empty() {
        render_wrapped_button_section(
            ui,
            app,
            right,
            button_size,
            BUTTON_SPACING,
            buttons_per_row,
            true,
        );
        if !left.is_empty() {
            ui.add_space(BUTTON_SPACING);
        }
    }
    if !left.is_empty() {
        render_wrapped_button_section(
            ui,
            app,
            left,
            button_size,
            BUTTON_SPACING,
            buttons_per_row,
            false,
        );
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
    left_button_count: usize,
    right_button_count: usize,
) -> VerticalPrimaryActionsLayout {
    let total_button_count = left_button_count + right_button_count;
    let total_button_width = row_width(total_button_count, button_width, button_spacing);
    let max_group_button_count = left_button_count.max(right_button_count).max(1);
    let max_group_width = row_width(max_group_button_count, button_width, button_spacing);
    let two_button_width = row_width(2, button_width, button_spacing);

    if available_width >= total_button_width {
        VerticalPrimaryActionsLayout::SingleRow
    } else if available_width >= max_group_width {
        VerticalPrimaryActionsLayout::CaptionFirstRows {
            buttons_per_row: max_group_button_count,
        }
    } else if available_width >= two_button_width {
        VerticalPrimaryActionsLayout::CaptionFirstRows { buttons_per_row: 2 }
    } else {
        VerticalPrimaryActionsLayout::CaptionFirstRows { buttons_per_row: 1 }
    }
}

struct VerticalActionButton {
    icon: &'static str,
    tooltip: Cow<'static, str>,
    action: VerticalAction,
}

impl VerticalActionButton {
    fn new(
        icon: &'static str,
        tooltip: impl Into<Cow<'static, str>>,
        action: VerticalAction,
    ) -> Self {
        Self {
            icon,
            tooltip: tooltip.into(),
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

impl VerticalAction {
    fn id_key(self) -> &'static str {
        match self {
            Self::OpenFile => "vertical_primary_open_file",
            Self::SaveFile => "vertical_primary_save_file",
            Self::Search => "vertical_primary_search",
            Self::Minimize => "vertical_caption_minimize",
            Self::ToggleMaximize => "vertical_caption_maximize",
            Self::CloseWindow => "vertical_caption_close",
        }
    }
}

fn handle_vertical_action(ctx: &egui::Context, app: &mut ScratchpadApp, action: VerticalAction) {
    match action {
        VerticalAction::OpenFile => {
            crate::app::commands::handle_command(app, AppCommand::File(FileCommand::OpenFile));
        }
        VerticalAction::SaveFile => {
            crate::app::commands::handle_command(app, AppCommand::File(FileCommand::SaveFile));
        }
        VerticalAction::Search => {
            crate::app::commands::handle_command(app, AppCommand::Search(SearchCommand::Toggle));
        }
        VerticalAction::Minimize => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        VerticalAction::ToggleMaximize => {
            let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        VerticalAction::CloseWindow => frame::request_exit(app, ctx),
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
        render_button(ui, app, button, button_size);
    }
}

fn render_button(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    button: &VerticalActionButton,
    button_size: egui::Vec2,
) {
    let is_close = matches!(button.action, VerticalAction::CloseWindow);
    let (background, hover_background) = if is_close {
        (action_bg(ui), CLOSE_HOVER_BG)
    } else {
        (action_bg(ui), action_hover_bg(ui))
    };
    let response = if is_close {
        phosphor_button_with_hover_icon_color(
            ui,
            button.action.id_key(),
            button.icon,
            button_size,
            PhosphorButtonColors::with_hover_icon(
                background,
                hover_background,
                crate::app::theme::text_primary(ui),
                egui::Color32::WHITE,
            ),
            &button.tooltip,
        )
    } else {
        phosphor_button(
            ui,
            button.action.id_key(),
            button.icon,
            button_size,
            background,
            hover_background,
            &button.tooltip,
        )
    };
    if response.clicked() {
        handle_vertical_action(ui.ctx(), app, button.action);
    }
}
