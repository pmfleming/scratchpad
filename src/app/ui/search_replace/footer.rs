//! Bottom panel for replace actions, reviews and outcomes; measured like History's.
use super::state::{SearchStripActions, SearchStripState};
use crate::app::{
    app_state::{ScratchpadApp, SearchStatus},
    theme::text_muted,
    ui::{notice, settings::widgets::group_heading},
};
use eframe::egui;

pub(super) fn panel(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    let key = egui::Id::unique("search.footer.height");
    let height = ui.ctx().data(|d| d.get_temp::<f32>(key)).unwrap_or(0.0);
    let measured = egui::Panel::bottom(egui::Id::unique("search.footer"))
        .resizable(false)
        .show_separator_line(false)
        .exact_size(height)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let top = ui.cursor().top();
            show(ui, app, state, actions);
            ui.cursor().top() - top
        })
        .inner;
    // Measure real wrapped content; a discarded sizing pass fits the footer before painting.
    if (measured - height).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(key, measured));
        ui.ctx()
            .request_discard("Search footer content changed size");
    }
}

fn show(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    if let Some(outcome) = &app.state.search_state.replace.outcome {
        notice::show(ui, egui_phosphor::regular::INFO, outcome, "");
        if let Some((operation, files)) = app.state.search_state.replace.last_operation {
            let allowed = crate::app::app_state::search_state::replace::transaction::can_undo(
                app, operation, files,
            );
            if ui.add_enabled(allowed, egui::Button::new("Undo all")).on_hover_text("Undo every file in this operation. All affected files must be open and at this edit in History.").clicked() { actions.undo_all = Some((operation, files)); }
        }
    }
    if let Some(review) = &app
        .state
        .search_state
        .replace
        .pending_replace_all_confirmation
    {
        group_heading(ui, "Review replace all");
        egui::ScrollArea::vertical()
            .id_salt("search.replace.review")
            .max_height(144.0)
            .show_rows(ui, 24.0, review.plan.targets.len(), |ui, range| {
                for target in &review.plan.targets[range] {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&target.buffer_label)
                                .size(12.5)
                                .color(text_muted(ui)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(format!("{} matches", target.replacements.len()));
                        });
                    });
                }
            });
        ui.horizontal(|ui| {
            actions.replace_all_requested |= ui
                .button(format!(
                    "Replace {} in {} files",
                    review.total_match_count, review.affected_buffer_count
                ))
                .clicked();
            actions.cancel_review |= ui.button("Cancel").clicked();
        });
    } else if state.replace_open
        && state.progress.status
            == SearchStatus::Error(
                crate::app::app_state::search_state::SEARCH_RESULT_LIMIT_MESSAGE.into(),
            )
    {
        actions.replace_all_requested |= ui
            .add_enabled(
                app.state.search_state.replace.planning.is_none()
                    && app.state.search_state.replace.pending.is_none(),
                egui::Button::new("Review all matches beyond the result limit"),
            )
            .clicked();
    } else if state.replace_open {
        ui.separator();
        let included = state
            .match_count
            .saturating_sub(app.state.search_state.replace.excluded.len());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{included} included"))
                    .size(12.5)
                    .color(text_muted(ui)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                actions.replace_all_requested |= ui
                    .add_enabled(
                        included > 0 && state.replace_availability.allows_actions(),
                        egui::Button::new("Replace all"),
                    )
                    .accessible_name("Replace all included matches")
                    .on_hover_text(crate::app::shortcut_tooltips::REPLACE_ALL_MATCHES)
                    .clicked();
                actions.replace_current_requested |= ui
                    .add_enabled(
                        state.replace_availability.allows_actions(),
                        egui::Button::new("Replace"),
                    )
                    .on_hover_text(crate::app::shortcut_tooltips::REPLACE_CURRENT_MATCH)
                    .clicked();
            });
        });
    }
}
