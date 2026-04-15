use super::shared::{collect_slot_entries, slot_cell_context};
use super::{DuplicateNameCounts, apply_tab_drag_feedback};
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::theme::{BUTTON_SIZE, TAB_BUTTON_WIDTH, action_bg, border};
use crate::app::ui::tab_drag::{self, TabDropAxis, TabDropZone};
use crate::app::ui::tab_strip::TabStripOutcome;
use eframe::egui::{self, Stroke};

pub(super) fn show_vertical_tab_region(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &DuplicateNameCounts,
) -> TabStripOutcome {
    tab_drag::sync_drag_state(ui);
    let mut outcome = TabStripOutcome::default();

    super::super::actions::show_vertical_primary_actions(ui, app);
    ui.add_space(8.0);
    let drop_zones =
        show_vertical_tab_entries_above_new_tab(ui, app, duplicate_name_counts, &mut outcome);
    apply_tab_drag_feedback(ui, app, &drop_zones, &mut outcome);
    outcome
}

fn show_vertical_tab_entries_above_new_tab(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &DuplicateNameCounts,
    outcome: &mut TabStripOutcome,
) -> Vec<TabDropZone> {
    let scroll_height = (ui.available_height() - BUTTON_SIZE.y - 8.0).max(0.0);
    let drop_zones = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), scroll_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| show_scrolling_vertical_tab_list(ui, app, duplicate_name_counts, outcome),
        )
        .inner
        .into_iter()
        .collect::<Vec<_>>();

    ui.add_space(8.0);
    show_vertical_new_tab_action(ui, app);
    drop_zones
}

fn show_vertical_new_tab_action(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let width = ui.available_width().max(BUTTON_SIZE.x);
    if ui
        .add_sized(
            egui::vec2(width, BUTTON_SIZE.y),
            egui::Button::new(format!("{} New tab", egui_phosphor::regular::PLUS))
                .fill(action_bg(ui))
                .stroke(Stroke::new(1.0, border(ui))),
        )
        .on_hover_text("New Tab")
        .clicked()
    {
        app.handle_command(AppCommand::NewTab);
    }
}

fn show_scrolling_vertical_tab_list(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &DuplicateNameCounts,
    outcome: &mut TabStripOutcome,
) -> Option<TabDropZone> {
    let scroll_area_id = ui.id().with("vertical_tab_list");
    let entries = egui::ScrollArea::vertical()
        .id_salt(scroll_area_id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let viewport_rect = ui.max_rect();
            maybe_auto_scroll_vertical_tab_list(ui, app, scroll_area_id, viewport_rect);
            collect_vertical_tab_entries(ui, app, duplicate_name_counts, outcome)
        })
        .inner;

    (!entries.is_empty()).then_some(TabDropZone {
        axis: TabDropAxis::Vertical,
        entries,
    })
}

fn collect_vertical_tab_entries(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &DuplicateNameCounts,
    outcome: &mut TabStripOutcome,
) -> Vec<crate::app::ui::tab_drag::TabRectEntry> {
    let context = slot_cell_context(
        app,
        duplicate_name_counts,
        ui.available_width().max(TAB_BUTTON_WIDTH),
    );
    collect_slot_entries(ui, app, &context, outcome, |_, _| {})
}

fn maybe_auto_scroll_vertical_tab_list(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    scroll_area_id: egui::Id,
    viewport_rect: egui::Rect,
) {
    if let Some(scroll_state) = egui::scroll_area::State::load(ui.ctx(), scroll_area_id) {
        crate::app::ui::tab_drag::auto_scroll_tab_list(
            ui.ctx(),
            scroll_area_id,
            viewport_rect,
            estimated_vertical_tab_list_height(app, 4.0),
            &scroll_state,
            crate::app::ui::tab_drag::TabDropAxis::Vertical,
        );
    }
}

fn estimated_vertical_tab_list_height(app: &ScratchpadApp, spacing: f32) -> f32 {
    let tab_count = app.total_tab_slots();
    if tab_count > 0 {
        (tab_count as f32 * crate::app::theme::TAB_HEIGHT)
            + ((tab_count.saturating_sub(1)) as f32 * spacing)
    } else {
        0.0
    }
}