//! Title and scope switch. The window never narrows below the full header.
use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::SearchScope;
use crate::app::shortcut_tooltips;
use crate::app::ui::panel_style::window;
use eframe::egui;

/// The narrowest window content that shows the title and every scope on one row.
pub(super) const WIDTH: window::MeasuredWidth = window::MeasuredWidth {
    key: "search.header.min_width",
    estimate: 420.0,
    discard_reason: "Search header width changed",
};

const SCOPES: [(SearchScope, &str, &str); 4] = [
    (
        SearchScope::SelectionOnly,
        "Selection",
        shortcut_tooltips::SEARCH_SCOPE_SELECTION,
    ),
    (
        SearchScope::ActiveBuffer,
        "File",
        shortcut_tooltips::SEARCH_SCOPE_CURRENT_FILE,
    ),
    (
        SearchScope::ActiveWorkspaceTab,
        "Tab",
        shortcut_tooltips::SEARCH_SCOPE_CURRENT_TAB,
    ),
    (
        SearchScope::AllOpenTabs,
        "All tabs",
        shortcut_tooltips::SEARCH_SCOPE_ALL_TABS,
    ),
];

/// Returns true when the close button was clicked.
pub(super) fn show(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    show_close: bool,
) -> bool {
    let close = show_close.then_some(("search.close", "Close search"));
    let header = window::header(ui, "Search", close, |switch| {
        for (scope, label, hint) in SCOPES.into_iter().rev() {
            let selected = state.scope == scope;
            let response = switch.ui.selectable_label(selected, label).on_hover_text(
                if scope == SearchScope::SelectionOnly && selected {
                    shortcut_tooltips::SEARCH_SCOPE_SELECTION_DEFAULT
                } else {
                    hint
                },
            );
            switch.measure(&response);
            if response.clicked() {
                state.scope = scope;
                actions.recapture_selection = scope == SearchScope::SelectionOnly;
            }
        }
    });
    WIDTH.remember(ui.ctx(), header.required_width);
    header.close
}
