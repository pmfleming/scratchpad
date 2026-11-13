//! Title and scope switch. The window never narrows below the full header.
use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::SearchScope;
use crate::app::shortcut_tooltips;
use crate::app::ui::{callout, panel_style};
use eframe::egui;

/// Used until the header has been measured once.
const ESTIMATED_MIN_WIDTH: f32 = 420.0;
/// Keeps the title visibly separate from the scope switch at minimum width.
const TITLE_GAP: f32 = 16.0;

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

fn min_width_key() -> egui::Id {
    egui::Id::unique("search.header.min_width")
}

/// The narrowest window content that shows the title and every scope on one row.
pub(super) fn min_width(ctx: &egui::Context) -> f32 {
    ctx.data(|d| d.get_temp::<f32>(min_width_key()))
        .unwrap_or(ESTIMATED_MIN_WIDTH)
}

/// Returns true when the close button was clicked.
pub(super) fn show(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    show_close: bool,
) -> bool {
    let spacing = ui.spacing().item_spacing.x;
    let mut close = false;
    let mut required = TITLE_GAP;
    // Size the row for the scope buttons first so the title centres against them.
    let height =
        ui.spacing().interact_size.y.max(
            ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y,
        );
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            required += ui
                .label(
                    egui::RichText::new("Search")
                        .size(14.0)
                        .strong()
                        .color(callout::text(ui)),
                )
                .rect
                .width();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if show_close {
                    let button = panel_style::icon_button(
                        ui,
                        "search.close",
                        egui_phosphor::regular::X,
                        "Close search",
                        true,
                    );
                    required += button.rect.width() + spacing;
                    close = button.clicked();
                }
                for (scope, label, hint) in SCOPES.into_iter().rev() {
                    let selected = state.scope == scope;
                    let response = ui.selectable_label(selected, label).on_hover_text(
                        if scope == SearchScope::SelectionOnly && selected {
                            shortcut_tooltips::SEARCH_SCOPE_SELECTION_DEFAULT
                        } else {
                            hint
                        },
                    );
                    required += response.rect.width() + spacing;
                    if response.clicked() {
                        state.scope = scope;
                        actions.recapture_selection = scope == SearchScope::SelectionOnly;
                    }
                }
            });
        },
    );
    remember_min_width(ui.ctx(), required.ceil());
    close
}

fn remember_min_width(ctx: &egui::Context, required: f32) {
    let changed = ctx
        .data(|d| d.get_temp::<f32>(min_width_key()))
        .is_none_or(|old| (old - required).abs() > 0.5);
    if changed {
        ctx.data_mut(|d| d.insert_temp(min_width_key(), required));
        // Font or theme changes alter the header; resize before it is painted.
        ctx.request_discard("Search header width changed");
    }
}
