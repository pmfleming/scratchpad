//! Bottom panel for replace actions, reviews and outcomes; measured like History's.
use super::result_model::{file_count_label, match_count_label};
use super::state::{SearchStripActions, SearchStripState};
use crate::app::{
    app_state::{ScratchpadApp, SearchStatus, search_state::SEARCH_RESULT_LIMIT_MESSAGE},
    shortcut_tooltips, theme,
    theme::text_muted,
    ui::panel_style::window,
};
use eframe::egui;

pub(super) fn panel(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    window::measured_footer(
        ui,
        "search.footer",
        0.0,
        "Search footer content changed size",
        |ui| show(ui, app, state, actions),
    );
}

const HINT_SIZE: f32 = 11.0;
const KEY_HINTS: &str = "↑ ↓ Move   Enter Go   Home End Jump   Esc Close";

fn show(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    ui.separator();
    // A pending review replaces the footer, like History's forget confirmation.
    if review(ui, app, actions) {
        return;
    }
    outcome(ui, app, actions);
    if state.replace_open {
        replace_actions(ui, app, state, actions);
    }
    ui.label(egui::RichText::new(KEY_HINTS).size(HINT_SIZE).weak());
}

fn review(ui: &mut egui::Ui, app: &ScratchpadApp, actions: &mut SearchStripActions) -> bool {
    let Some(review) = &app
        .state
        .search_state
        .replace
        .pending_replace_all_confirmation
    else {
        return false;
    };
    ui.add(
        egui::Label::new(
            egui::RichText::new(format!(
                "Replace {} in {}?",
                match_count_label(review.total_match_count),
                file_count_label(review.affected_buffer_count)
            ))
            .strong(),
        )
        .truncate(),
    );
    let files = review
        .plan
        .targets
        .iter()
        .map(|target| target.buffer_label.as_str())
        .collect::<Vec<_>>();
    ui.label(
        egui::RichText::new(file_list(&files))
            .size(HINT_SIZE + 1.0)
            .color(text_muted(ui)),
    );
    ui.label(
        egui::RichText::new("Excluded matches are kept. Undo all reverts every file at once.")
            .size(HINT_SIZE + 1.0)
            .color(text_muted(ui)),
    );
    ui.horizontal_wrapped(|ui| {
        actions.cancel_review |= ui.button("Cancel").clicked();
        let primary = window::primary_button(
            ui,
            format!(
                "Replace {} in {}",
                review.total_match_count,
                file_count_label(review.affected_buffer_count)
            ),
            true,
        );
        actions.replace_all_requested |= ui.add(primary).clicked();
    });
    true
}

/// Up to three file names, then a count of the rest.
fn file_list(files: &[&str]) -> String {
    const SHOWN: usize = 3;
    let names = files
        .iter()
        .map(|path| {
            std::path::Path::new(path)
                .file_name()
                .map_or(*path, |name| name.to_str().unwrap_or(path))
        })
        .take(SHOWN)
        .collect::<Vec<_>>()
        .join(", ");
    match files.len().saturating_sub(SHOWN) {
        0 => names,
        more => format!("{names} and {more} more"),
    }
}

fn outcome(ui: &mut egui::Ui, app: &ScratchpadApp, actions: &mut SearchStripActions) {
    let Some(outcome) = &app.state.search_state.replace.outcome else {
        return;
    };
    ui.horizontal(|ui| {
        if let Some((operation, files)) = app.state.search_state.replace.last_operation {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let allowed = crate::app::app_state::search_state::replace::transaction::can_undo(
                    app, operation, files,
                );
                let undo = ui
                    .add_enabled(
                        allowed,
                        egui::Button::new(egui::RichText::new("Undo all").size(HINT_SIZE + 1.0))
                            .frame(false),
                    )
                    .on_hover_text(
                        "Undo every file in this operation. All affected files must be open and at this edit in History.",
                    );
                if undo.clicked() {
                    actions.undo_all = Some((operation, files));
                }
                outcome_label(ui, outcome);
            });
        } else {
            outcome_label(ui, outcome);
        }
    });
}

fn outcome_label(ui: &mut egui::Ui, outcome: &str) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.label(
            egui::RichText::new(egui_phosphor::regular::CHECK)
                .font(egui_phosphor::font_id(HINT_SIZE + 3.0))
                .color(theme::diff_add(ui)),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(outcome)
                    .size(HINT_SIZE + 1.0)
                    .color(text_muted(ui)),
            )
            .truncate(),
        );
    });
}

fn replace_actions(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    let limited = state.progress.status == SearchStatus::Error(SEARCH_RESULT_LIMIT_MESSAGE.into());
    let replace = &app.state.search_state.replace;
    let included = state.match_count.saturating_sub(replace.excluded.len());
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if limited {
                actions.replace_all_requested |= ui
                    .add_enabled(
                        replace.planning.is_none() && replace.pending.is_none(),
                        egui::Button::new("Review all matches"),
                    )
                    .on_hover_text("Plan Replace all for every match, beyond the result limit")
                    .clicked();
                return;
            }
            let allowed = state.replace_availability.allows_actions();
            actions.replace_all_requested |= ui
                .add_enabled(included > 0 && allowed, egui::Button::new("Replace all"))
                .accessible_name("Replace all included matches")
                .on_hover_text(shortcut_tooltips::REPLACE_ALL_MATCHES)
                .clicked();
            actions.replace_current_requested |= ui
                .add_enabled(allowed, egui::Button::new("Replace"))
                .on_hover_text(shortcut_tooltips::REPLACE_CURRENT_MATCH)
                .clicked();
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{included} included"))
                        .size(HINT_SIZE + 1.0)
                        .color(text_muted(ui)),
                );
            });
        });
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_list_names_three_files_then_counts_the_rest() {
        assert_eq!(super::file_list(&["/a/one.txt"]), "one.txt");
        assert_eq!(
            super::file_list(&["/a/one.txt", "two.txt", "three.txt", "four", "five"]),
            "one.txt, two.txt, three.txt and 2 more"
        );
    }
}
