use super::common::{history_dialog_card, history_dialog_header, show_centered_callout};
use super::text_history::{
    HISTORY_PILL_CORNER_RADIUS, HISTORY_PILL_INNER_MARGIN, HISTORY_PILL_SPACING,
    TEXT_HISTORY_LIST_MIN_HEIGHT, TEXT_HISTORY_SIZE, truncated_label,
};
use crate::app::app_state::{
    DialogState, StatusDomain, StatusMessage, StatusSeverity, StatusState,
};
use crate::app::theme::{action_bg, border, tab_selected_accent, tab_selected_bg};
use crate::app::ui::{callout, settings, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{BRACKETS_SQUARE, EXCLAMATION_MARK, QUESTION_MARK, X};

const STATUS_TITLE_SIZE: f32 = 24.0;
const STATUS_FILTER_BUTTON_HEIGHT: f32 = 30.0;
const STATUS_PILL_ICON_SIZE: f32 = 16.0;
const STATUS_CARD_CORNER_RADIUS: u8 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusFilter {
    All,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StatusHistoryRow {
    pub(super) id: u64,
    pub(super) severity: StatusSeverity,
    pub(super) domain: StatusDomain,
    pub(super) text: String,
    pub(super) detail: Option<String>,
}

impl StatusHistoryRow {
    fn from_status(status: &StatusMessage) -> Self {
        Self {
            id: status.id,
            severity: status.severity,
            domain: status.domain,
            text: status.text.clone(),
            detail: status.detail.clone(),
        }
    }
}

pub(crate) fn show_status_history_window(
    ctx: &egui::Context,
    dialogs: &mut DialogState,
    status: &StatusState,
) {
    if !dialogs.status_history.is_open() {
        return;
    }

    let rows = status_history_rows(status.history.iter());
    let filter = read_filter(ctx);
    let mut next_filter = filter;
    let mut close_requested = false;

    show_centered_callout(ctx, "status_history_window", TEXT_HISTORY_SIZE, |ui| {
        widget_ids::feature_scope(ui, "status_history_dialog", |ui| {
            render_status_history_window(ui, &rows, filter, &mut next_filter, &mut close_requested);
        });
    });

    if next_filter != filter {
        write_filter(ctx, next_filter);
    }
    if close_requested {
        dialogs.status_history.close();
    }
}

fn status_history_rows<'a>(
    messages: impl DoubleEndedIterator<Item = &'a StatusMessage>,
) -> Vec<StatusHistoryRow> {
    messages.rev().map(StatusHistoryRow::from_status).collect()
}

fn render_status_history_window(
    ui: &mut egui::Ui,
    rows: &[StatusHistoryRow],
    filter: StatusFilter,
    next_filter: &mut StatusFilter,
    close_requested: &mut bool,
) {
    settings::apply_dialog_typography(ui);
    callout::apply_spacing(ui);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 12.0);
    if render_header(ui) {
        *close_requested = true;
    }

    ui.add_space(4.0);
    history_dialog_card(ui, STATUS_CARD_CORNER_RADIUS, |ui| {
        render_filter_controls(ui, filter, next_filter)
    });

    let filtered_rows = rows
        .iter()
        .filter(|row| filter.matches(row.severity))
        .collect::<Vec<_>>();
    render_history_rows(ui, &filtered_rows);
}

fn render_header(ui: &mut egui::Ui) -> bool {
    history_dialog_header(
        ui,
        "status_history.header",
        "Close status",
        "Status",
        STATUS_TITLE_SIZE,
    )
}

fn render_filter_controls(ui: &mut egui::Ui, filter: StatusFilter, next_filter: &mut StatusFilter) {
    let filters = [
        (
            "all",
            BRACKETS_SQUARE,
            "All status messages",
            StatusFilter::All,
        ),
        ("info", QUESTION_MARK, "Info messages", StatusFilter::Info),
        (
            "warning",
            EXCLAMATION_MARK,
            "Warning messages",
            StatusFilter::Warning,
        ),
        ("error", X, "Error messages", StatusFilter::Error),
    ];

    ui.horizontal(|ui| {
        for (id_source, icon, tooltip, candidate) in filters {
            if control_icon_button(ui, id_source, icon, tooltip, filter == candidate).clicked() {
                *next_filter = candidate;
            }
        }
    });
}

fn control_icon_button(
    ui: &mut egui::Ui,
    id_source: &'static str,
    icon: &str,
    tooltip: &str,
    selected: bool,
) -> egui::Response {
    let (fill, stroke_color, text_color) = if selected {
        (
            tab_selected_bg(ui),
            tab_selected_accent(ui),
            callout::text(ui),
        )
    } else {
        (action_bg(ui), border(ui), callout::muted_text(ui))
    };
    let button = egui::Button::new(
        egui::RichText::new(icon)
            .font(egui::FontId::proportional(16.0))
            .color(text_color),
    )
    .min_size(egui::vec2(36.0, STATUS_FILTER_BUTTON_HEIGHT))
    .fill(fill)
    .stroke(egui::Stroke::new(1.0, stroke_color))
    .corner_radius(egui::CornerRadius::same(8));
    widget_ids::scope(ui, ("status_history.filter", id_source), |ui| {
        ui.add(button)
    })
    .inner
    .on_hover_text(tooltip)
}

fn render_history_rows(ui: &mut egui::Ui, rows: &[&StatusHistoryRow]) {
    widget_ids::scope(ui, "status_history.rows", |ui| {
        if rows.is_empty() {
            ui.label(
                egui::RichText::new("No status messages")
                    .size(13.0)
                    .color(callout::muted_text(ui)),
            );
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt(widget_ids::ctx_key("status_history.scroll.content"))
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .min_scrolled_height(TEXT_HISTORY_LIST_MIN_HEIGHT)
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                for row in rows {
                    render_status_history_row(ui, row);
                }
            });
    });
}

fn render_status_history_row(ui: &mut egui::Ui, row: &StatusHistoryRow) {
    widget_ids::scope(ui, ("status_history.row", row.id), |ui| {
        status_pill(ui, row);
    });
    ui.add_space(HISTORY_PILL_SPACING);
}

fn status_pill(ui: &mut egui::Ui, row: &StatusHistoryRow) {
    egui::Frame::NONE
        .fill(action_bg(ui))
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(HISTORY_PILL_CORNER_RADIUS))
        .inner_margin(egui::Margin::same(HISTORY_PILL_INNER_MARGIN))
        .show(ui, |ui| render_row_pill_contents(ui, row));
}

fn render_row_pill_contents(ui: &mut egui::Ui, row: &StatusHistoryRow) {
    let content_width = ui.available_width();
    ui.set_width(content_width);
    ui.set_min_width(content_width);
    ui.set_max_width(content_width);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(severity_icon(row.severity))
                .font(egui::FontId::proportional(STATUS_PILL_ICON_SIZE))
                .color(severity_color(row.severity, ui.visuals().dark_mode)),
        );
        ui.add_space(8.0);
        ui.vertical(|ui| {
            let text_width = ui.available_width().max(0.0);
            truncated_label(
                ui,
                &row.text,
                text_width,
                14.0,
                callout::text(ui),
                egui::Sense::hover(),
            )
            .on_hover_text(&row.text);
            truncated_label(
                ui,
                domain_label(row.domain),
                text_width,
                12.0,
                callout::muted_text(ui),
                egui::Sense::hover(),
            );
            if let Some(detail) = &row.detail {
                egui::CollapsingHeader::new("Detail")
                    .id_salt(("status_history.detail", row.id))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(detail)
                                .size(12.0)
                                .monospace()
                                .color(callout::muted_text(ui)),
                        );
                    });
            }
        });
    });
}

fn severity_icon(severity: StatusSeverity) -> &'static str {
    match severity {
        StatusSeverity::Info => QUESTION_MARK,
        StatusSeverity::Warning => EXCLAMATION_MARK,
        StatusSeverity::Error => X,
    }
}

fn severity_color(severity: StatusSeverity, dark_mode: bool) -> egui::Color32 {
    match severity {
        StatusSeverity::Info => {
            if dark_mode {
                egui::Color32::from_rgb(170, 178, 190)
            } else {
                egui::Color32::from_rgb(82, 92, 108)
            }
        }
        StatusSeverity::Warning => egui::Color32::from_rgb(230, 132, 46),
        StatusSeverity::Error => egui::Color32::from_rgb(220, 64, 64),
    }
}

fn domain_label(domain: StatusDomain) -> &'static str {
    match domain {
        StatusDomain::File => "File",
        StatusDomain::Disk => "Disk",
        StatusDomain::Search => "Search",
        StatusDomain::Settings => "Settings",
        StatusDomain::Session => "Session",
        StatusDomain::Encoding => "Encoding",
        StatusDomain::History => "History",
        StatusDomain::Layout => "Layout",
    }
}

impl StatusFilter {
    fn matches(self, severity: StatusSeverity) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Info => severity == StatusSeverity::Info,
            StatusFilter::Warning => severity == StatusSeverity::Warning,
            StatusFilter::Error => severity == StatusSeverity::Error,
        }
    }
}

fn read_filter(ctx: &egui::Context) -> StatusFilter {
    ctx.data_mut(|data| {
        data.get_persisted::<StatusFilter>(filter_id())
            .unwrap_or(StatusFilter::All)
    })
}

fn write_filter(ctx: &egui::Context, filter: StatusFilter) {
    ctx.data_mut(|data| data.insert_persisted(filter_id(), filter));
}

fn filter_id() -> egui::Id {
    widget_ids::ctx_key("status_history.filter")
}

#[cfg(test)]
mod tests {
    use super::{StatusFilter, StatusHistoryRow, status_history_rows};
    use crate::app::app_state::{StatusDomain, StatusMessage, StatusSeverity};
    #[test]
    fn status_history_rows_are_newest_first() {
        let older = message(1, "Older");
        let newer = message(2, "Newer");

        let rows = status_history_rows([older, newer].iter());

        assert_eq!(
            rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
            vec!["Newer", "Older"]
        );
    }

    #[test]
    fn status_history_row_keeps_detail_separate_from_primary_text() {
        let mut status = message(1, "Could not save your session.");
        status.detail = Some("access denied".to_owned());

        let row = StatusHistoryRow::from_status(&status);

        assert_eq!(row.text, "Could not save your session.");
        assert_eq!(row.detail.as_deref(), Some("access denied"));
        assert!(!row.text.contains("access denied"));
    }

    #[test]
    fn status_filter_matches_by_severity() {
        assert!(StatusFilter::All.matches(StatusSeverity::Error));
        assert!(StatusFilter::Warning.matches(StatusSeverity::Warning));
        assert!(!StatusFilter::Warning.matches(StatusSeverity::Info));
    }

    fn message(id: u64, text: &str) -> StatusMessage {
        StatusMessage {
            id,
            severity: StatusSeverity::Info,
            domain: StatusDomain::Disk,
            text: text.to_owned(),
            detail: None,
        }
    }
}
