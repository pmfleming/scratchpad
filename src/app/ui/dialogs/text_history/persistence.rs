use super::HistoryTab;
use crate::app::ui::widget_ids;
use eframe::egui;

pub(super) fn read_active_tab(ctx: &egui::Context) -> HistoryTab {
    widget_ids::read_deferred_persisted::<u8>(
        ctx,
        widget_ids::ctx_key("text_history.active_tab.pending"),
        widget_ids::ctx_key("text_history.active_tab"),
    )
    .and_then(tab_from_persisted)
    .unwrap_or(HistoryTab::ByFile)
}

pub(super) fn write_active_tab(ctx: &egui::Context, tab: HistoryTab) {
    let pending_id = widget_ids::ctx_key("text_history.active_tab.pending");
    widget_ids::write_deferred_persisted(ctx, pending_id, tab_to_persisted(tab));
}

pub(super) fn read_follow_focus(ctx: &egui::Context) -> bool {
    widget_ids::read_deferred_persisted::<bool>(
        ctx,
        widget_ids::ctx_key("text_history.follow_undo.pending"),
        widget_ids::ctx_key("text_history.follow_undo"),
    )
    .unwrap_or(true)
}

pub(super) fn write_follow_focus(ctx: &egui::Context, follow: bool) {
    let pending_id = widget_ids::ctx_key("text_history.follow_undo.pending");
    widget_ids::write_deferred_persisted(ctx, pending_id, follow);
}

fn tab_from_persisted(value: u8) -> Option<HistoryTab> {
    match value {
        0 => Some(HistoryTab::Timeline),
        1 => Some(HistoryTab::ByFile),
        _ => None,
    }
}

fn tab_to_persisted(tab: HistoryTab) -> u8 {
    match tab {
        HistoryTab::Timeline => 0,
        HistoryTab::ByFile => 1,
    }
}
