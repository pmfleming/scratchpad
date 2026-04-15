use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferFreshness, PendingAction};
use crate::app::services::file_controller::FileController;
use crate::app::services::file_service::COMMON_TEXT_ENCODINGS;
use eframe::egui;
use egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE;

struct EncodingDialogState {
    active_index: usize,
    has_saved_path: bool,
    is_dirty: bool,
    current_encoding_label: String,
}

struct SaveConflictDialogState {
    title: &'static str,
    message: String,
    freshness: BufferFreshness,
}

pub(crate) fn show_pending_action_modal(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(action) = app.pending_action() else {
        return;
    };

    match action {
        PendingAction::CloseTab(index) => {
            if !pending_close_tab_is_valid(app, index) {
                clear_pending_action(app);
                return;
            }

            if !app.tabs()[index].buffer.is_dirty {
                close_pending_tab(app, index);
                return;
            }

            show_close_tab_confirmation(ctx, app, index);
        }
        PendingAction::SaveConflict(index) => {
            if !pending_save_conflict_is_valid(app, index) {
                clear_pending_action(app);
                return;
            }

            show_save_conflict_confirmation(ctx, app, index);
        }
    }
}

pub(crate) fn show_transaction_log_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.transaction_log_open() {
        return;
    }

    let mut open = true;
    let mut undo_entry_id = None;

    egui::Window::new("Transaction Log")
        .open(&mut open)
        .resizable(true)
        .default_size(egui::vec2(480.0, 320.0))
        .show(ctx, |ui| {
            render_transaction_log_contents(ui, app.transaction_log_entries(), &mut undo_entry_id);
        });

    if let Some(entry_id) = undo_entry_id {
        let _ = app.undo_transaction_entry(entry_id);
        return;
    }

    if !open {
        app.close_transaction_log();
    }
}

pub(crate) fn show_encoding_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.encoding_dialog_open {
        return;
    }

    let state = current_encoding_dialog_state(app);
    let mut close_window = false;

    show_modal_window(ctx, "Encoding", |ui| {
        ui.label(format!(
            "Current encoding: {}",
            state.current_encoding_label
        ));
        ui.separator();

        render_reopen_with_encoding_section(ui, app, &state, &mut close_window);

        ui.separator();
        render_save_with_encoding_section(ui, app, state.active_index, &mut close_window);

        ui.separator();
        if ui.button("Close").clicked() {
            close_window = true;
        }
    });

    if close_window {
        app.close_encoding_dialog();
    }
}

fn render_encoding_combo(ui: &mut egui::Ui, id: &str, selected: &mut String) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected.as_str())
        .show_ui(ui, |ui| {
            for option in COMMON_TEXT_ENCODINGS {
                ui.selectable_value(selected, option.canonical_name.to_owned(), option.label);
            }
        });
}

fn pending_close_tab_is_valid(app: &ScratchpadApp, index: usize) -> bool {
    index < app.tabs().len()
}

fn pending_save_conflict_is_valid(app: &ScratchpadApp, index: usize) -> bool {
    index < app.tabs().len()
        && app.tabs()[index].active_buffer().path.is_some()
        && !matches!(
            app.tabs()[index].active_buffer().freshness,
            BufferFreshness::InSync
        )
}

fn show_close_tab_confirmation(ctx: &egui::Context, app: &mut ScratchpadApp, index: usize) {
    let tab_name = app.tabs()[index].buffer.name.clone();

    show_modal_window(ctx, "Unsaved Changes", |ui| {
        ui.label(format!("Do you want to save changes to {}?", tab_name));
        ui.horizontal(|ui| {
            render_close_tab_confirmation_buttons(ui, app, index);
        });
    });
}

fn show_save_conflict_confirmation(ctx: &egui::Context, app: &mut ScratchpadApp, index: usize) {
    let Some(state) = save_conflict_dialog_state(app, index) else {
        return;
    };

    show_modal_window(ctx, state.title, |ui| {
        ui.label(&state.message);
        ui.horizontal(|ui| {
            render_save_conflict_buttons(ui, app, index, state.freshness);
        });
    });
}

fn render_close_tab_confirmation_buttons(ui: &mut egui::Ui, app: &mut ScratchpadApp, index: usize) {
    if ui.button("Save").clicked() {
        save_and_close_pending_tab(app, index);
    }
    if ui.button("Don't Save").clicked() {
        close_pending_tab(app, index);
    }
    if ui.button("Cancel").clicked() {
        clear_pending_action(app);
    }
}

fn save_and_close_pending_tab(app: &mut ScratchpadApp, index: usize) {
    if app.save_file_at(index) {
        close_pending_tab(app, index);
    }
}

fn close_pending_tab(app: &mut ScratchpadApp, index: usize) {
    clear_pending_action(app);
    app.handle_command(AppCommand::CloseTab { index });
}

fn clear_pending_action(app: &mut ScratchpadApp) {
    app.set_pending_action(None);
}

fn current_encoding_dialog_state(app: &ScratchpadApp) -> EncodingDialogState {
    let active_index = app.active_tab_index();
    let (has_saved_path, is_dirty, current_encoding_label) = app
        .active_tab()
        .map(|tab| {
            (
                tab.active_buffer().path.is_some(),
                tab.active_buffer().is_dirty,
                tab.active_buffer().format.encoding_label(),
            )
        })
        .unwrap_or_else(|| (false, false, "UTF-8".to_owned()));

    EncodingDialogState {
        active_index,
        has_saved_path,
        is_dirty,
        current_encoding_label,
    }
}

fn render_reopen_with_encoding_section(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &EncodingDialogState,
    close_window: &mut bool,
) {
    ui.label(egui::RichText::new("Reopen With Encoding").strong());
    if state.has_saved_path {
        render_encoding_combo(
            ui,
            "reopen_with_encoding_combo",
            &mut app.reopen_with_encoding_choice,
        );
        let response = ui.add_enabled(!state.is_dirty, egui::Button::new("Reopen With Encoding"));
        if response.clicked() {
            let selected_encoding = app.reopen_with_encoding_choice.clone();
            if FileController::reopen_buffer_with_encoding(
                app,
                state.active_index,
                &selected_encoding,
            ) {
                *close_window = true;
            }
        }
        if state.is_dirty {
            ui.label(
                egui::RichText::new(
                    "Save or discard changes before reopening with a different encoding.",
                )
                .color(egui::Color32::YELLOW),
            );
        }
    } else {
        ui.label("Reopen With Encoding is available only for files on disk.");
    }
}

fn render_save_with_encoding_section(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    active_index: usize,
    close_window: &mut bool,
) {
    ui.label(egui::RichText::new("Save With Encoding").strong());
    render_encoding_combo(
        ui,
        "save_with_encoding_combo",
        &mut app.save_with_encoding_choice,
    );
    if ui.button("Save With Encoding").clicked() {
        let selected_encoding = app.save_with_encoding_choice.clone();
        if FileController::save_file_with_encoding_at(app, active_index, &selected_encoding) {
            *close_window = true;
        }
    }
}

fn show_modal_window(
    ctx: &egui::Context,
    title: impl Into<egui::WidgetText>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, add_contents);
}

fn render_transaction_log_contents(
    ui: &mut egui::Ui,
    entries: &[crate::app::transactions::TransactionLogEntry],
    undo_entry_id: &mut Option<u64>,
) {
    if entries.is_empty() {
        ui.label("No undoable workspace transactions yet.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for entry in entries.iter().rev() {
            render_transaction_log_entry(ui, entry, undo_entry_id);
            ui.separator();
        }
    });
}

fn render_transaction_log_entry(
    ui: &mut egui::Ui,
    entry: &crate::app::transactions::TransactionLogEntry,
    undo_entry_id: &mut Option<u64>,
) {
    ui.vertical(|ui| {
        ui.horizontal_wrapped(|ui| {
            let undo_button = ui
                .button(ARROW_COUNTER_CLOCKWISE)
                .on_hover_text("Undo to this point");
            if undo_button.clicked() {
                *undo_entry_id = Some(entry.id);
            }
            ui.label(
                egui::RichText::new(&entry.action_label)
                    .monospace()
                    .strong(),
            );
            if !entry.affected_items.is_empty() {
                ui.label(egui::RichText::new(entry.affected_items.join(", ")).small());
            }
        });
        if let Some(details) = &entry.details {
            ui.label(egui::RichText::new(details).small().italics());
        }
    });
}

fn save_conflict_dialog_state(
    app: &ScratchpadApp,
    index: usize,
) -> Option<SaveConflictDialogState> {
    let buffer = app.tabs()[index].active_buffer();
    let path_label = buffer
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| buffer.name.clone());
    let freshness = buffer.freshness;
    let title = match freshness {
        BufferFreshness::MissingOnDisk => "File Missing on Disk",
        BufferFreshness::ConflictOnDisk | BufferFreshness::StaleOnDisk => "File Changed on Disk",
        BufferFreshness::InSync => return None,
    };
    let message = match freshness {
        BufferFreshness::ConflictOnDisk => {
            format!("{path_label} changed on disk while this tab has unsaved edits.")
        }
        BufferFreshness::MissingOnDisk => {
            format!("{path_label} is missing on disk, but this tab still has content.")
        }
        BufferFreshness::StaleOnDisk => format!("{path_label} changed on disk."),
        BufferFreshness::InSync => return None,
    };

    Some(SaveConflictDialogState {
        title,
        message,
        freshness,
    })
}

fn render_save_conflict_buttons(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    index: usize,
    freshness: BufferFreshness,
) {
    let primary_label = if freshness == BufferFreshness::MissingOnDisk {
        "Recreate"
    } else {
        "Overwrite"
    };
    if ui.button(primary_label).clicked() && FileController::save_conflict_overwrite(app, index) {
        clear_pending_action(app);
    }
    if freshness != BufferFreshness::MissingOnDisk
        && ui.button("Reload").clicked()
        && FileController::reload_buffer_from_disk(app, index)
    {
        clear_pending_action(app);
    }
    if ui.button("Save As Copy").clicked() && app.save_file_as_at(index) {
        clear_pending_action(app);
    }
    if ui.button("Cancel").clicked() {
        clear_pending_action(app);
    }
}
