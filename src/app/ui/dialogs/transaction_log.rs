use super::common::{relative_age_label, show_callout};
use crate::app::app_state::ScratchpadApp;
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::theme::{
    action_hover_bg, border, tab_selected_accent, tab_selected_bg, text_muted, text_primary,
};
use crate::app::transactions::TransactionLogEntry;
use crate::app::ui::{callout, search_replace, settings};
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_COUNTER_CLOCKWISE, ARROWS_SPLIT, CLOCK_COUNTER_CLOCKWISE, FILE_TEXT, PENCIL_SIMPLE_LINE,
    SLIDERS_HORIZONTAL,
};
use std::borrow::Cow;

const TRANSACTION_LOG_WIDTH: f32 = search_replace::SEARCH_DIALOG_WIDTH;
const TRANSACTION_LOG_SIZE: egui::Vec2 = egui::vec2(TRANSACTION_LOG_WIDTH, 560.0);
const TRANSACTION_LOG_TITLE_SIZE: f32 = 24.0;
const TRANSACTION_LOG_PANEL_CORNER_RADIUS: u8 = 12;
const TRANSACTION_LOG_FILTER_HEIGHT: f32 = 36.0;
const TRANSACTION_LOG_LIST_MAX_HEIGHT: f32 = 300.0;
const TRANSACTION_LOG_UNDO_BUTTON_SIZE: egui::Vec2 = egui::vec2(42.0, 38.0);
const TRANSACTION_LOG_SECTION_LABEL: &str = "TODAY";
const TRANSACTION_LOG_FILE_ICON: egui::Color32 = egui::Color32::from_rgb(238, 240, 244);
const TRANSACTION_LOG_MUTED_BLUE: egui::Color32 = egui::Color32::from_rgb(144, 198, 255);

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransactionFilter {
    All,
    FileChanges,
    TabOperations,
    Modifications,
}

impl TransactionFilter {
    const ALL: [Self; 4] = [
        Self::All,
        Self::FileChanges,
        Self::TabOperations,
        Self::Modifications,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::FileChanges => "File changes",
            Self::TabOperations => "Tab operations",
            Self::Modifications => "Modifications",
        }
    }

    fn matches_category(self, category: TransactionCategory) -> bool {
        matches!(self, Self::All)
            || matches!(
                (self, category),
                (Self::FileChanges, TransactionCategory::FileChanges)
                    | (Self::TabOperations, TransactionCategory::TabOperations)
                    | (Self::Modifications, TransactionCategory::Modifications)
            )
    }

    fn persisted_value(self) -> u8 {
        match self {
            Self::All => 0,
            Self::FileChanges => 1,
            Self::TabOperations => 2,
            Self::Modifications => 3,
        }
    }

    fn from_persisted_value(value: u8) -> Self {
        match value {
            1 => Self::FileChanges,
            2 => Self::TabOperations,
            3 => Self::Modifications,
            _ => Self::All,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransactionCategory {
    FileChanges,
    TabOperations,
    Modifications,
}

impl TransactionCategory {
    fn icon(self) -> &'static str {
        match self {
            Self::FileChanges => FILE_TEXT,
            Self::TabOperations => ARROWS_SPLIT,
            Self::Modifications => PENCIL_SIMPLE_LINE,
        }
    }

    fn from_entry(entry: &TransactionLogEntry) -> Self {
        let label = entry.action_label.to_ascii_lowercase();
        if contains_any(&label, &["tab", "split", "combine", "promote", "view"]) {
            Self::TabOperations
        } else if contains_any(
            &label,
            &["file", "save", "open", "reload", "reopen", "rename"],
        ) {
            Self::FileChanges
        } else {
            Self::Modifications
        }
    }
}

pub(super) fn show_transaction_log_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.transaction_log_open() {
        return;
    }

    let mut close_requested = false;
    let mut undo_entry_id = None;
    let mut clear_requested = false;

    show_callout(
        ctx,
        "transaction_log_overlay_v3",
        callout::centered_position(ctx, TRANSACTION_LOG_SIZE),
        TRANSACTION_LOG_SIZE.x,
        |ui| {
            render_transaction_log_window(
                ui,
                app.transaction_log_entries(),
                &mut undo_entry_id,
                &mut clear_requested,
                &mut close_requested,
            );
        },
    );

    if let Some(entry_id) = undo_entry_id {
        let _ = app.undo_transaction_entry(entry_id);
    } else if clear_requested {
        app.clear_transaction_log();
    } else if close_requested {
        app.close_transaction_log();
    }
}

fn render_transaction_log_window(
    ui: &mut egui::Ui,
    entries: &[TransactionLogEntry],
    undo_entry_id: &mut Option<u64>,
    clear_requested: &mut bool,
    close_requested: &mut bool,
) {
    settings::apply_dialog_typography(ui);
    apply_transaction_log_typography(ui);
    callout::apply_spacing(ui);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 12.0);

    if render_dialog_header(ui) {
        *close_requested = true;
    }

    ui.add_space(8.0);

    transaction_log_panel_frame(ui).show(ui, |ui| {
        let filter_id = ui.make_persistent_id("transaction_log_filter");
        let mut filter = load_transaction_filter(ui, filter_id);

        render_panel_intro(ui);
        ui.add_space(14.0);
        render_filter_row(
            ui,
            &mut filter,
            filter_id,
            !entries.is_empty(),
            clear_requested,
        );
        let filtered_entries = filtered_entries(entries, filter);
        ui.add_space(12.0);
        divider(ui);
        ui.add_space(12.0);

        if filtered_entries.is_empty() {
            render_empty_state(ui, entries.is_empty());
        } else {
            ui.label(
                egui::RichText::new(TRANSACTION_LOG_SECTION_LABEL)
                    .size(12.0)
                    .color(text_muted(ui))
                    .strong(),
            );
            ui.add_space(10.0);
            render_entry_list(ui, &filtered_entries, undo_entry_id);
            ui.add_space(10.0);
            divider(ui);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(entry_count_label(filtered_entries.len()))
                        .size(13.0)
                        .color(text_muted(ui)),
                );
            });
        }
    });
}

fn render_dialog_header(ui: &mut egui::Ui) -> bool {
    callout::header_row(ui, "Close transaction log", |ui| {
        ui.label(
            egui::RichText::new("Transaction log")
                .size(TRANSACTION_LOG_TITLE_SIZE)
                .color(callout::text(ui)),
        );
    })
}

fn render_panel_intro(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.allocate_ui(egui::vec2(28.0, 28.0), |ui| {
            ui.with_layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    ui.label(
                        egui::RichText::new(SLIDERS_HORIZONTAL)
                            .font(egui::FontId::proportional(18.0))
                            .color(text_primary(ui)),
                    );
                },
            );
        });
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("Filters")
                .size(18.0)
                .color(text_primary(ui)),
        );
    });
}

fn render_filter_row(
    ui: &mut egui::Ui,
    filter: &mut TransactionFilter,
    filter_id: egui::Id,
    can_clear: bool,
    clear_requested: &mut bool,
) {
    ui.horizontal(|ui| {
        for option in TransactionFilter::ALL {
            if filter_chip(ui, option.label(), *filter == option).clicked() {
                *filter = option;
                store_transaction_filter(ui, filter_id, option);
            }
            ui.add_space(4.0);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let clear = ui
                .add_enabled(
                    can_clear,
                    egui::Button::new(
                        egui::RichText::new("Clear history")
                            .size(13.0)
                            .color(TRANSACTION_LOG_MUTED_BLUE),
                    )
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE),
                )
                .on_hover_text("Remove all recorded transaction entries");
            if clear.clicked() {
                *clear_requested = true;
            }
        });
    });
}

fn filter_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let fill = if selected {
        tab_selected_bg(ui)
    } else {
        egui::Color32::TRANSPARENT
    };
    let stroke_color = if selected {
        tab_selected_accent(ui)
    } else {
        border(ui).gamma_multiply(0.85)
    };

    ui.add(
        egui::Button::new(egui::RichText::new(label).size(12.5).color(if selected {
            text_primary(ui)
        } else {
            text_primary(ui).gamma_multiply(0.9)
        }))
        .min_size(egui::vec2(0.0, TRANSACTION_LOG_FILTER_HEIGHT))
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .corner_radius(egui::CornerRadius::same(8)),
    )
}

fn render_empty_state(ui: &mut egui::Ui, log_is_empty: bool) {
    ui.add_space(32.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(CLOCK_COUNTER_CLOCKWISE)
                .font(egui::FontId::proportional(20.0))
                .color(text_muted(ui)),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(if log_is_empty {
                "No transaction entries yet."
            } else {
                "No entries match the current filter."
            })
            .size(14.0)
            .color(text_muted(ui)),
        );
    });
    ui.add_space(32.0);
}

fn render_entry_list(
    ui: &mut egui::Ui,
    entries: &[&TransactionLogEntry],
    undo_entry_id: &mut Option<u64>,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(TRANSACTION_LOG_LIST_MAX_HEIGHT)
        .show(ui, |ui| {
            for entry in entries.iter().rev() {
                render_entry_row(ui, entry, undo_entry_id);
                ui.add_space(8.0);
            }
        });
}

fn render_entry_row(
    ui: &mut egui::Ui,
    entry: &TransactionLogEntry,
    undo_entry_id: &mut Option<u64>,
) {
    let category = TransactionCategory::from_entry(entry);
    let tokens = transaction_log_tokens(entry);
    transaction_entry_frame(ui).show(ui, |ui| {
        ui.horizontal(|ui| {
            render_row_icon(ui, category);
            ui.add_space(12.0);

            let meta_width = 130.0;
            let content_width = (ui.available_width() - meta_width - 40.0).max(160.0);
            ui.allocate_ui_with_layout(
                egui::vec2(content_width, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(&entry.action_label)
                            .size(14.5)
                            .color(text_primary(ui)),
                    );
                    if !tokens.is_empty() {
                        ui.add_space(6.0);
                        render_entry_pills(ui, &tokens);
                    }
                },
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                let undo = callout::icon_button(
                    ui,
                    ARROW_COUNTER_CLOCKWISE,
                    18.0,
                    TRANSACTION_LOG_UNDO_BUTTON_SIZE,
                    action_hover_bg(ui),
                    "Undo to this point",
                    true,
                );
                if undo.clicked() {
                    *undo_entry_id = Some(entry.id);
                }
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{} ago",
                        relative_age_label(entry.created_at.elapsed())
                    ))
                    .size(12.5)
                    .color(text_muted(ui)),
                );
            });
        });
    });
}

fn render_row_icon(ui: &mut egui::Ui, category: TransactionCategory) {
    ui.allocate_ui(egui::vec2(26.0, 26.0), |ui| {
        ui.with_layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.label(
                    egui::RichText::new(category.icon())
                        .font(egui::FontId::proportional(20.0))
                        .color(TRANSACTION_LOG_FILE_ICON),
                );
            },
        );
    });
}

fn filtered_entries(
    entries: &[TransactionLogEntry],
    filter: TransactionFilter,
) -> Vec<&TransactionLogEntry> {
    entries
        .iter()
        .filter(|entry| filter.matches_category(TransactionCategory::from_entry(entry)))
        .collect()
}

fn transaction_log_tokens(entry: &TransactionLogEntry) -> Vec<Cow<'_, str>> {
    let mut tokens = entry
        .affected_items
        .iter()
        .map(|item| Cow::Borrowed(item.as_str()))
        .collect::<Vec<_>>();

    if let Some(details) = entry
        .details
        .as_deref()
        .filter(|details| !details.is_empty())
    {
        tokens.push(Cow::Borrowed(details));
    }

    tokens
}

fn render_entry_pills(ui: &mut egui::Ui, tokens: &[Cow<'_, str>]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for token in tokens {
            egui::Frame::NONE
                .fill(if ui.visuals().dark_mode {
                    action_hover_bg(ui)
                } else {
                    tab_selected_bg(ui)
                })
                .stroke(egui::Stroke::new(1.0, border(ui).gamma_multiply(0.9)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(token.as_ref())
                            .size(12.0)
                            .color(text_muted(ui)),
                    );
                });
        }
    });
}

fn entry_count_label(entry_count: usize) -> String {
    if entry_count == 1 {
        "1 entry".to_owned()
    } else {
        format!("{entry_count} entries")
    }
}

fn apply_transaction_log_typography(ui: &mut egui::Ui) {
    let font_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
    let style = ui.style_mut();
    style.override_font_id = Some(egui::FontId::new(15.0, font_family.clone()));
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(15.0, font_family.clone()),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, font_family.clone()),
    );
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::new(12.0, font_family));
}

fn transaction_log_panel_frame(ui: &egui::Ui) -> egui::Frame {
    settings::dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(
            TRANSACTION_LOG_PANEL_CORNER_RADIUS,
        ))
        .inner_margin(egui::Margin::symmetric(18, 16))
}

fn transaction_entry_frame(ui: &egui::Ui) -> egui::Frame {
    settings::dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 6))
}

fn divider(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, border(ui).gamma_multiply(0.85));
}

fn load_transaction_filter(ui: &mut egui::Ui, id: egui::Id) -> TransactionFilter {
    TransactionFilter::from_persisted_value(
        ui.data_mut(|data| data.get_persisted::<u8>(id))
            .unwrap_or_default(),
    )
}

fn store_transaction_filter(ui: &mut egui::Ui, id: egui::Id, filter: TransactionFilter) {
    ui.data_mut(|data| data.insert_persisted(id, filter.persisted_value()));
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}
