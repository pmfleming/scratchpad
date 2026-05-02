mod horizontal;
mod shared;
mod vertical;

use super::{HeaderLayout, TabStripOutcome};
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::WorkspaceTab;
use crate::app::ui::tab_drag::{self, TabDropZone};
use eframe::egui;
use std::collections::HashMap;

type DuplicateNameCounts = HashMap<String, usize>;

pub(crate) fn duplicate_name_counts(tabs: &[WorkspaceTab]) -> DuplicateNameCounts {
    let mut counts = HashMap::with_capacity(tabs.len());
    for tab in tabs {
        *counts.entry(tab.buffer.name.clone()).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn show_vertical_tab_region(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
) -> TabStripOutcome {
    let duplicate_name_counts = duplicate_name_counts(app.tabs());
    vertical::show_vertical_tab_region(ui, app, &duplicate_name_counts)
}

pub(crate) fn show_tab_region(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
) -> TabStripOutcome {
    let duplicate_name_counts = duplicate_name_counts(app.tabs());
    horizontal::show_tab_region(ctx, ui, app, layout, &duplicate_name_counts)
}

fn apply_tab_drag_feedback(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    drop_zones: &[TabDropZone],
    outcome: &mut TabStripOutcome,
) {
    update_reordered_tabs(ui, app.total_tab_slots(), drop_zones, outcome);
    tab_drag::paint_dragged_tab_ghost(ui.ctx(), app);
}

fn update_reordered_tabs(
    ui: &mut egui::Ui,
    tab_count: usize,
    drop_zones: &[TabDropZone],
    outcome: &mut TabStripOutcome,
) {
    if let Some(commit) = tab_drag::update_tab_drag(ui, drop_zones, tab_count) {
        match commit {
            tab_drag::TabDragCommit::Reorder {
                from_index,
                to_index,
            } => outcome.reordered_tabs = Some((from_index, to_index)),
            tab_drag::TabDragCommit::ReorderGroup {
                from_indices,
                to_index,
            } => outcome.reordered_tab_group = Some((from_indices, to_index)),
            tab_drag::TabDragCommit::Combine {
                source_index,
                target_index,
            } => outcome.combined_tabs = Some((source_index, target_index)),
            tab_drag::TabDragCommit::CombineGroup {
                source_indices,
                target_index,
            } => outcome.combined_tab_group = Some((source_indices, target_index)),
        }
    }
}
