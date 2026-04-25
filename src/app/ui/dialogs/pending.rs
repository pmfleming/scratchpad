use super::common::show_callout;
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferFreshness, PendingAction, ViewId};
use crate::app::services::file_controller::FileController;
use crate::app::theme::CAPTION_BUTTON_SIZE;
use crate::app::ui::callout;
use eframe::egui;
use egui_phosphor::regular::{ARROW_CLOCKWISE, COPY, FILE_TEXT, FLOPPY_DISK, TRASH, WARNING, X};

const UNSAVED_CHANGES_SIZE: egui::Vec2 = egui::vec2(272.0, 154.0);
const UNSAVED_CHANGES_ACTION_BUTTON_SIZE: egui::Vec2 = egui::vec2(72.0, 54.0);
const MISSING_FILE_DIALOG_SIZE: egui::Vec2 = egui::vec2(432.0, 154.0);
const SAVE_CONFLICT_DIALOG_SIZE: egui::Vec2 = egui::vec2(432.0, 214.0);

struct SaveConflictDialogState {
    title: &'static str,
    message: String,
    path_label: String,
    freshness: BufferFreshness,
}

#[derive(Clone, Copy)]
enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

impl SaveConflictDialogState {
    fn from_freshness(path_label: String, freshness: BufferFreshness) -> Option<Self> {
        let (title, message) = match freshness {
            BufferFreshness::ConflictOnDisk => (
                "File Changed on Disk",
                format!("{path_label} changed on disk while this tab has unsaved edits."),
            ),
            BufferFreshness::MissingOnDisk => (
                "File Missing on Disk",
                format!("{path_label} is missing on disk, but this tab still has content."),
            ),
            BufferFreshness::StaleOnDisk => (
                "File Changed on Disk",
                format!("{path_label} changed on disk."),
            ),
            BufferFreshness::InSync => return None,
        };

        Some(Self {
            title,
            message,
            path_label,
            freshness,
        })
    }

    fn primary_action_label(&self) -> &'static str {
        "Overwrite"
    }

    fn can_reload(&self) -> bool {
        self.freshness != BufferFreshness::MissingOnDisk
    }

    fn is_missing_on_disk(&self) -> bool {
        self.freshness == BufferFreshness::MissingOnDisk
    }
}

pub(super) fn show_pending_action_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(action) = app.pending_action() else {
        return;
    };

    match action {
        PendingAction::CloseTab(index) => handle_pending_close_tab(ctx, app, index),
        PendingAction::CloseView { tab_index, view_id } => {
            handle_pending_close_view(ctx, app, tab_index, view_id)
        }
        PendingAction::SaveConflict { tab_index, view_id }
            if save_conflict_dialog_state(app, tab_index, view_id).is_some() =>
        {
            show_save_conflict_confirmation(ctx, app, tab_index, view_id)
        }
        PendingAction::SaveConflict { .. } => clear_pending_action(app),
    }
}

fn handle_pending_close_tab(ctx: &egui::Context, app: &mut ScratchpadApp, index: usize) {
    match app.tabs().get(index) {
        None => clear_pending_action(app),
        Some(tab) if !tab.buffer.is_dirty => close_pending_tab(app, index),
        Some(_) => show_close_tab_confirmation(ctx, app, index),
    }
}

fn handle_pending_close_view(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) {
    let Some(tab) = app.tabs().get(tab_index) else {
        clear_pending_action(app);
        return;
    };

    if tab.is_last_view_for_buffer(view_id) != Some(true) {
        close_pending_view(app, tab_index, view_id);
        return;
    }

    match tab.buffer_for_view(view_id) {
        None => clear_pending_action(app),
        Some(buffer) if !buffer.is_dirty => close_pending_view(app, tab_index, view_id),
        Some(_) => show_close_view_confirmation(ctx, app, tab_index, view_id),
    }
}

fn show_close_tab_confirmation(ctx: &egui::Context, app: &mut ScratchpadApp, index: usize) {
    let tab_name = app.tabs()[index].buffer.name.clone();
    let mut close_requested = false;

    show_callout(
        ctx,
        "unsaved_changes_overlay_v3",
        callout::centered_position(ctx, UNSAVED_CHANGES_SIZE),
        UNSAVED_CHANGES_SIZE.x,
        |ui| render_unsaved_changes_dialog(ui, &tab_name, app, index, &mut close_requested),
    );

    if close_requested {
        clear_pending_action(app);
    }
}

fn show_close_view_confirmation(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) {
    let Some(tab_name) = app
        .tabs()
        .get(tab_index)
        .and_then(|tab| tab.buffer_for_view(view_id))
        .map(|buffer| buffer.name.clone())
    else {
        clear_pending_action(app);
        return;
    };
    let mut close_requested = false;

    show_callout(
        ctx,
        "unsaved_changes_overlay_v3",
        callout::centered_position(ctx, UNSAVED_CHANGES_SIZE),
        UNSAVED_CHANGES_SIZE.x,
        |ui| {
            render_unsaved_changes_view_dialog(
                ui,
                &tab_name,
                app,
                tab_index,
                view_id,
                &mut close_requested,
            )
        },
    );

    if close_requested {
        clear_pending_action(app);
    }
}

fn show_save_conflict_confirmation(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) {
    let Some(state) = save_conflict_dialog_state(app, tab_index, view_id) else {
        return;
    };

    let mut close_requested = false;
    let dialog_size = if state.is_missing_on_disk() {
        MISSING_FILE_DIALOG_SIZE
    } else {
        SAVE_CONFLICT_DIALOG_SIZE
    };

    show_callout(
        ctx,
        "file_change_overlay_v1",
        callout::centered_position(ctx, dialog_size),
        dialog_size.x,
        |ui| render_save_conflict_dialog(ui, app, tab_index, view_id, &state, &mut close_requested),
    );

    if close_requested {
        clear_pending_action(app);
    }
}

fn render_unsaved_changes_view_dialog(
    ui: &mut egui::Ui,
    tab_name: &str,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    close_requested: &mut bool,
) {
    match render_unsaved_changes_body(ui, tab_name, close_requested) {
        Some(UnsavedChoice::Save) => save_and_close_pending_view(app, tab_index, view_id),
        Some(UnsavedChoice::Discard) => close_pending_view(app, tab_index, view_id),
        Some(UnsavedChoice::Cancel) => *close_requested = true,
        None => {}
    }
}

fn render_unsaved_changes_dialog(
    ui: &mut egui::Ui,
    tab_name: &str,
    app: &mut ScratchpadApp,
    index: usize,
    close_requested: &mut bool,
) {
    match render_unsaved_changes_body(ui, tab_name, close_requested) {
        Some(UnsavedChoice::Save) => save_and_close_pending_tab(app, index),
        Some(UnsavedChoice::Discard) => close_pending_tab(app, index),
        Some(UnsavedChoice::Cancel) => *close_requested = true,
        None => {}
    }
}

fn render_unsaved_changes_body(
    ui: &mut egui::Ui,
    tab_name: &str,
    close_requested: &mut bool,
) -> Option<UnsavedChoice> {
    render_icon_choice_dialog(
        ui,
        tab_name,
        "Unsaved Changes",
        close_requested,
        [
            (FLOPPY_DISK, "Save changes", UnsavedChoice::Save),
            (TRASH, "Discard changes", UnsavedChoice::Discard),
            (X, "Cancel", UnsavedChoice::Cancel),
        ],
    )
}

fn render_unsaved_changes_header(ui: &mut egui::Ui, tab_name: &str) -> bool {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), CAPTION_BUTTON_SIZE.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(FILE_TEXT)
                    .size(16.0)
                    .color(callout::muted_text(ui)),
            );
            ui.add_space(6.0);

            let label_width = (ui.available_width() - CAPTION_BUTTON_SIZE.x - 6.0).max(0.0);
            let label = truncate_unsaved_title(ui, tab_name, label_width);
            ui.add_sized(
                egui::vec2(label_width, 0.0),
                egui::Label::new(
                    egui::RichText::new(label)
                        .size(15.0)
                        .monospace()
                        .color(callout::text(ui)),
                ),
            );

            callout::close_button(ui, "Cancel").clicked()
        },
    )
    .inner
}

fn truncate_unsaved_title(ui: &egui::Ui, tab_name: &str, max_width: f32) -> String {
    let marker = "...";
    let font_id = egui::FontId::monospace(15.0);

    if text_width(ui, tab_name, font_id.clone()) <= max_width {
        return tab_name.to_owned();
    }
    if text_width(ui, marker, font_id.clone()) >= max_width {
        return marker.to_owned();
    }

    let chars = tab_name.chars().collect::<Vec<_>>();
    let mut prefix_len = chars.len().saturating_sub(1);

    loop {
        let prefix = chars[..prefix_len].iter().collect::<String>();
        let candidate = format!("{prefix}{marker}");

        if text_width(ui, &candidate, font_id.clone()) <= max_width {
            return candidate;
        }

        if prefix_len > 1 {
            prefix_len -= 1;
        } else {
            return marker.to_owned();
        }
    }
}

fn text_width(ui: &egui::Ui, text: &str, font_id: egui::FontId) -> f32 {
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font_id, callout::text(ui))
            .size()
            .x
    })
}

fn save_and_close_pending_tab(app: &mut ScratchpadApp, index: usize) {
    if app.save_file_at(index) {
        close_pending_tab(app, index);
    }
}

fn save_and_close_pending_view(app: &mut ScratchpadApp, tab_index: usize, view_id: ViewId) {
    if !activate_pending_view(app, tab_index, view_id) {
        clear_pending_action(app);
        return;
    }

    if app.save_file_at(tab_index) {
        close_pending_view(app, tab_index, view_id);
    }
}

fn render_save_conflict_dialog(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    state: &SaveConflictDialogState,
    close_requested: &mut bool,
) {
    if state.is_missing_on_disk() {
        render_missing_file_dialog(ui, app, tab_index, view_id, state, close_requested);
        return;
    }

    callout::apply_spacing(ui);

    if callout::header_row(ui, "Close file change prompt", |ui| {
        ui.label(
            egui::RichText::new(WARNING)
                .size(16.0)
                .color(callout::muted_text(ui)),
        );
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(state.title)
                    .size(15.0)
                    .color(callout::text(ui)),
            );
            ui.label(
                egui::RichText::new("Resolve the on-disk mismatch before continuing.")
                    .size(11.5)
                    .color(callout::muted_text(ui)),
            );
        });
    }) {
        *close_requested = true;
    }

    callout::section_frame(ui).show(ui, |ui| {
        ui.label(
            egui::RichText::new(&state.message)
                .size(12.5)
                .color(callout::text(ui)),
        );
    });

    ui.horizontal_wrapped(|ui| {
        if render_save_conflict_button(
            ui,
            FLOPPY_DISK,
            state.primary_action_label(),
            "Write the current buffer back to disk",
        ) && run_save_conflict_action(app, tab_index, view_id, |app, tab_index| {
            FileController::save_conflict_overwrite(app, tab_index)
        }) {
            clear_pending_action(app);
        }

        if state.can_reload()
            && render_save_conflict_button(
                ui,
                ARROW_CLOCKWISE,
                "Reload",
                "Discard local buffer state and reload from disk",
            )
            && run_save_conflict_action(app, tab_index, view_id, |app, tab_index| {
                FileController::reload_buffer_from_disk(app, tab_index)
            })
        {
            clear_pending_action(app);
        }

        if render_save_conflict_button(
            ui,
            COPY,
            "Save As Copy",
            "Keep this buffer by saving it to a new file",
        ) && run_save_conflict_action(app, tab_index, view_id, ScratchpadApp::save_file_as_at)
        {
            clear_pending_action(app);
        }

        if render_save_conflict_button(ui, X, "Cancel", "Dismiss this prompt") {
            *close_requested = true;
        }
    });
}

fn render_missing_file_dialog(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    state: &SaveConflictDialogState,
    close_requested: &mut bool,
) {
    if let Some(action) = render_icon_choice_dialog(
        ui,
        &state.path_label,
        "File Missing on Disk",
        close_requested,
        [
            (
                FLOPPY_DISK,
                "Recreate the file at its original path",
                MissingFileChoice::Save,
            ),
            (
                TRASH,
                "Discard this missing file tab",
                MissingFileChoice::Discard,
            ),
        ],
    ) {
        match action {
            MissingFileChoice::Save => {
                if run_save_conflict_action(app, tab_index, view_id, |app, tab_index| {
                    FileController::save_conflict_overwrite(app, tab_index)
                }) {
                    clear_pending_action(app);
                }
            }
            MissingFileChoice::Discard => close_pending_view(app, tab_index, view_id),
        }
    }
}

fn render_icon_choice_dialog<T: Copy, const N: usize>(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    close_requested: &mut bool,
    actions: [(&str, &str, T); N],
) -> Option<T> {
    callout::apply_spacing(ui);
    ui.spacing_mut().item_spacing = egui::vec2(10.0, 12.0);

    if render_unsaved_changes_header(ui, title) {
        *close_requested = true;
    }

    ui.add_space(2.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(subtitle)
                .size(12.0)
                .color(callout::muted_text(ui)),
        );
    });

    ui.add_space(2.0);
    render_icon_choice_actions(ui, actions)
}

fn render_icon_choice_actions<T: Copy, const N: usize>(
    ui: &mut egui::Ui,
    actions: [(&str, &str, T); N],
) -> Option<T> {
    let mut selected = None;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
        for (icon, tooltip, action) in actions {
            if callout::icon_button(
                ui,
                icon,
                26.0,
                UNSAVED_CHANGES_ACTION_BUTTON_SIZE,
                callout::section_fill(ui),
                tooltip,
                true,
            )
            .clicked()
            {
                selected = Some(action);
            }
        }
    });
    selected
}

#[derive(Clone, Copy)]
enum MissingFileChoice {
    Save,
    Discard,
}

fn render_save_conflict_button(ui: &mut egui::Ui, icon: &str, label: &str, tooltip: &str) -> bool {
    ui.add(
        egui::Button::new(
            egui::RichText::new(format!("{icon} {label}"))
                .size(12.0)
                .color(callout::text(ui)),
        )
        .fill(callout::section_fill(ui))
        .corner_radius(egui::CornerRadius::same(8))
        .min_size(egui::vec2(98.0, 34.0)),
    )
    .on_hover_text(tooltip)
    .clicked()
}

fn close_pending_tab(app: &mut ScratchpadApp, index: usize) {
    clear_pending_action(app);
    app.handle_command(AppCommand::CloseTab { index });
}

fn close_pending_view(app: &mut ScratchpadApp, tab_index: usize, view_id: ViewId) {
    clear_pending_action(app);
    if activate_pending_view(app, tab_index, view_id) {
        app.perform_close_view(view_id);
    }
}

fn run_save_conflict_action(
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    action: impl FnOnce(&mut ScratchpadApp, usize) -> bool,
) -> bool {
    activate_pending_view(app, tab_index, view_id) && action(app, tab_index)
}

fn activate_pending_view(app: &mut ScratchpadApp, tab_index: usize, view_id: ViewId) -> bool {
    if tab_index >= app.tabs().len() {
        return false;
    }

    if app.active_tab_index() != tab_index {
        app.handle_command(AppCommand::ActivateTab { index: tab_index });
    }

    let Some(tab) = app.tabs().get(tab_index) else {
        return false;
    };
    if tab.view(view_id).is_none() {
        return false;
    }

    if tab.active_view_id != view_id {
        app.handle_command(AppCommand::ActivateView { view_id });
    }

    app.tabs()
        .get(tab_index)
        .is_some_and(|tab| tab.active_view_id == view_id)
}

fn clear_pending_action(app: &mut ScratchpadApp) {
    app.set_pending_action(None);
}

fn save_conflict_dialog_state(
    app: &ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
) -> Option<SaveConflictDialogState> {
    let buffer = app.tabs().get(tab_index)?.buffer_for_view(view_id)?;
    let path_label = buffer
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| buffer.name.clone());
    SaveConflictDialogState::from_freshness(path_label, buffer.freshness)
}
