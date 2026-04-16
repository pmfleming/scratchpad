use super::common::{relative_age_label, show_callout};
use crate::app::app_state::ScratchpadApp;
use crate::app::theme::action_bg;
use crate::app::transactions::TransactionLogEntry;
use crate::app::ui::callout;
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_COUNTER_CLOCKWISE, ARROWS_SPLIT, FILE_TEXT, PENCIL_SIMPLE_LINE, SLIDERS_HORIZONTAL,
};
use std::borrow::Cow;

const TRANSACTION_LOG_WIDTH: f32 = 520.0;
const TRANSACTION_LOG_LIST_MAX_HEIGHT: f32 = 520.0;
const TRANSACTION_LOG_CARD_MIN_HEIGHT: f32 = 56.0;
const TRANSACTION_LOG_META_WIDTH: f32 = 76.0;
const TRANSACTION_LOG_CLOSE_BUTTON_SIZE: egui::Vec2 = egui::vec2(34.0, 34.0);
const TRANSACTION_LOG_UNDO_BUTTON_SIZE: egui::Vec2 = egui::vec2(30.0, 30.0);
const TRANSACTION_LOG_HEADER_LEFT_WIDTH: f32 = 72.0;

pub(super) fn show_transaction_log_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.transaction_log_open() {
        return;
    }

    let mut close_requested = false;
    let mut undo_entry_id = None;

    show_callout(
        ctx,
        "transaction_log_overlay_v2",
        callout::top_left_position(ctx, ctx.content_rect().left()),
        TRANSACTION_LOG_WIDTH,
        |ui| {
            render_transaction_log_window(
                ui,
                app.transaction_log_entries(),
                &mut undo_entry_id,
                &mut close_requested,
            );
        },
    );

    if let Some(entry_id) = undo_entry_id {
        let _ = app.undo_transaction_entry(entry_id);
    } else if close_requested {
        app.close_transaction_log();
    }
}

fn render_transaction_log_window(
    ui: &mut egui::Ui,
    entries: &[TransactionLogEntry],
    undo_entry_id: &mut Option<u64>,
    close_requested: &mut bool,
) {
    callout::apply_spacing(ui);
    ui.spacing_mut().item_spacing.y = 10.0;

    render_transaction_log_header(ui, entries.len(), close_requested);

    if entries.is_empty() {
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            ui.add_space(18.0);
            ui.label(
                egui::RichText::new(SLIDERS_HORIZONTAL)
                    .size(18.0)
                    .color(callout::muted_text(ui)),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("No undoable workspace transactions yet.")
                    .size(15.0)
                    .color(callout::muted_text(ui)),
            );
        });
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(TRANSACTION_LOG_LIST_MAX_HEIGHT)
        .show(ui, |ui| {
            for entry in entries.iter().rev() {
                render_transaction_log_card(ui, entry, undo_entry_id);
                ui.add_space(4.0);
            }
        });
}

fn render_transaction_log_header(
    ui: &mut egui::Ui,
    entry_count: usize,
    close_requested: &mut bool,
) {
    let title_width = (ui.available_width()
        - TRANSACTION_LOG_HEADER_LEFT_WIDTH
        - TRANSACTION_LOG_CLOSE_BUTTON_SIZE.x
        - 8.0)
        .max(0.0);

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), TRANSACTION_LOG_CLOSE_BUTTON_SIZE.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(
                    TRANSACTION_LOG_HEADER_LEFT_WIDTH,
                    TRANSACTION_LOG_CLOSE_BUTTON_SIZE.y,
                ),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new(SLIDERS_HORIZONTAL)
                            .size(18.0)
                            .color(callout::muted_text(ui)),
                    );
                    callout::badge(ui, &entry_count.to_string());
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(title_width, TRANSACTION_LOG_CLOSE_BUTTON_SIZE.y),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    ui.label(
                        egui::RichText::new("Transaction Log")
                            .size(17.0)
                            .color(callout::text(ui)),
                    );
                },
            );

            if callout::close_button(ui, "Close transaction log").clicked() {
                *close_requested = true;
            }
        },
    );
}

fn render_transaction_log_card(
    ui: &mut egui::Ui,
    entry: &TransactionLogEntry,
    undo_entry_id: &mut Option<u64>,
) {
    callout::section_frame(ui).show(ui, |ui| {
        ui.set_min_height(TRANSACTION_LOG_CARD_MIN_HEIGHT);
        let content_width = (ui.available_width() - TRANSACTION_LOG_META_WIDTH - 8.0).max(80.0);
        let subtitle = transaction_log_subtitle(entry);

        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(content_width, TRANSACTION_LOG_CARD_MIN_HEIGHT - 20.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new(transaction_log_icon(entry))
                            .size(18.0)
                            .color(callout::text(ui)),
                    );
                    ui.add_space(10.0);
                    let title_response = ui.add_sized(
                        egui::vec2((content_width - 28.0).max(0.0), 0.0),
                        egui::Label::new(
                            egui::RichText::new(&entry.action_label)
                                .size(15.0)
                                .color(callout::text(ui)),
                        )
                        .truncate(),
                    );
                    if let Some(subtitle) = subtitle {
                        title_response.on_hover_text(subtitle.as_ref());
                    }
                },
            );

            ui.allocate_ui_with_layout(
                egui::vec2(
                    TRANSACTION_LOG_META_WIDTH,
                    TRANSACTION_LOG_CARD_MIN_HEIGHT - 20.0,
                ),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let undo_response = callout::icon_button(
                        ui,
                        ARROW_COUNTER_CLOCKWISE,
                        18.0,
                        TRANSACTION_LOG_UNDO_BUTTON_SIZE,
                        action_bg(ui),
                        "Undo to this point",
                        true,
                    );
                    if undo_response.clicked() {
                        *undo_entry_id = Some(entry.id);
                    }
                    ui.label(
                        egui::RichText::new(relative_age_label(entry.created_at.elapsed()))
                            .size(12.0)
                            .color(callout::muted_text(ui)),
                    );
                },
            );
        });
    });
}

fn transaction_log_icon(entry: &TransactionLogEntry) -> &'static str {
    let label = entry.action_label.to_ascii_lowercase();
    if label.contains("promote") || label.contains("tab") && label.contains("new") {
        FILE_TEXT
    } else if label.contains("combine") || label.contains("split") || label.contains("view") {
        ARROWS_SPLIT
    } else {
        PENCIL_SIMPLE_LINE
    }
}

fn transaction_log_subtitle(entry: &TransactionLogEntry) -> Option<Cow<'_, str>> {
    if !entry.affected_items.is_empty() {
        Some(entry.affected_items.join(", ").into())
    } else {
        entry
            .details
            .as_deref()
            .filter(|details| !details.is_empty())
            .map(Into::into)
    }
}
