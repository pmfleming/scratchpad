use crate::app::domain::WorkspaceTab;
use crate::app::chrome::tab_button;
use crate::app::ui::tab_drag;
use eframe::egui;
use std::collections::HashMap;

pub(crate) struct TabCellOutcome {
    pub interaction: TabInteraction,
    pub rect: egui::Rect,
}

pub(crate) enum TabInteraction {
    None,
    Activate(usize),
    RequestClose(usize),
}

pub(crate) fn render_tab_cell(
    ui: &mut egui::Ui,
    index: usize,
    tab: &WorkspaceTab,
    is_active: bool,
    pending_scroll_to_active: bool,
    duplicate_name_counts: &HashMap<String, usize>,
) -> TabCellOutcome {
    ui.push_id(("tab_strip", index), |ui| {
        let has_duplicate = duplicate_name_counts
            .get(&tab.buffer.name)
            .copied()
            .unwrap_or(0)
            > 1;
        let display_name = tab.full_display_name(has_duplicate);

        let (tab_response, close_response, truncated) = tab_button(ui, &display_name, is_active);
        let tab_response = maybe_attach_tab_tooltip(tab_response, tab, truncated);
        tab_drag::begin_tab_drag_if_needed(ui, index, &tab_response, &close_response);

        if is_active && pending_scroll_to_active {
            tab_response.scroll_to_me(Some(egui::Align::Center));
        }

        let interaction = if close_response.clicked() {
            TabInteraction::RequestClose(index)
        } else if tab_response.clicked() {
            TabInteraction::Activate(index)
        } else {
            TabInteraction::None
        };

        TabCellOutcome {
            interaction,
            rect: tab_response.rect,
        }
    })
    .inner
}

fn maybe_attach_tab_tooltip(
    tab_response: egui::Response,
    tab: &WorkspaceTab,
    truncated: bool,
) -> egui::Response {
    if truncated {
        tab_response.on_hover_text(tab.display_name())
    } else {
        tab_response
    }
}
