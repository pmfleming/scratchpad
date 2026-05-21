use super::shared::{
    attach_tab_list_background_context_menu, collect_slot_entries, slot_cell_context,
};
use super::{DuplicateNameCounts, apply_tab_drag_feedback};
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::{AppCommand, WorkspaceCommand};
use crate::app::shortcut_tooltips;
use crate::app::theme::{
    BUTTON_SIZE, TAB_BUTTON_WIDTH, action_bg, action_hover_bg, border, tab_list_scroll_style,
    text_primary,
};
use crate::app::ui::tab_drag::{self, TabDropAxis, TabDropZone};
use crate::app::ui::tab_strip::{
    TabStripOutcome, maybe_auto_scroll_vertical_tab_list as auto_scroll_vertical_tab_list,
};
use crate::app::ui::transition;
use crate::app::ui::widget_ids;
use eframe::egui::{self, Sense, Stroke};

pub(super) fn show_vertical_tab_region(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &DuplicateNameCounts,
) -> TabStripOutcome {
    let mut outcome = TabStripOutcome::default();

    widget_ids::feature_scope(ui, "tab_strip_vertical", |ui| {
        tab_drag::sync_drag_state(ui);
        super::super::actions::show_vertical_primary_actions(ui, app);
        ui.add_space(8.0);
        let drop_zones =
            show_vertical_tab_entries_above_new_tab(ui, app, duplicate_name_counts, &mut outcome);
        apply_tab_drag_feedback(ui, app, duplicate_name_counts, &drop_zones, &mut outcome);
    });
    outcome
}

fn show_vertical_tab_entries_above_new_tab(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &DuplicateNameCounts,
    outcome: &mut TabStripOutcome,
) -> Vec<TabDropZone> {
    let scroll_height = (ui.available_height() - BUTTON_SIZE.y - 8.0).max(0.0);
    let output = ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), scroll_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| show_scrolling_vertical_tab_list(ui, app, duplicate_name_counts, outcome),
    );
    let drop_zones = output.inner.into_iter().collect::<Vec<_>>();
    let entries = drop_zones
        .iter()
        .flat_map(|zone| zone.entries.iter())
        .copied()
        .collect::<Vec<_>>();
    attach_tab_list_background_context_menu(
        ui,
        output.response.rect,
        app,
        &entries,
        "vertical_tab_list_background_context",
    );

    ui.add_space(8.0);
    show_vertical_new_tab_action(ui, app);
    drop_zones
}

fn show_vertical_new_tab_action(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let width = ui.available_width().max(BUTTON_SIZE.x);
    if vertical_new_tab_button(ui, width)
        .on_hover_text(shortcut_tooltips::NEW_TAB)
        .clicked()
    {
        crate::app::commands::handle_command(app, AppCommand::Workspace(WorkspaceCommand::NewTab));
    }
}

fn vertical_new_tab_button(ui: &mut egui::Ui, width: f32) -> egui::Response {
    let response = widget_ids::allocate_exact_interact(
        ui,
        egui::vec2(width, BUTTON_SIZE.y),
        widget_ids::surface_role("vertical_new_tab", widget_ids::WidgetRole::ActionButton),
        Sense::click(),
        "vertical_new_tab",
    );
    let hovered = response.hovered() && !transition::suppress_interactive_chrome(ui.ctx());
    let fill = if hovered {
        action_hover_bg(ui)
    } else {
        action_bg(ui)
    };

    ui.painter().rect_filled(response.rect, 4.0, fill);
    ui.painter().rect_stroke(
        response.rect,
        4.0,
        Stroke::new(1.0, border(ui)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        response.rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{} New tab", egui_phosphor::regular::PLUS),
        egui::TextStyle::Button.resolve(ui.style()),
        text_primary(ui),
    );

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn show_scrolling_vertical_tab_list(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &DuplicateNameCounts,
    outcome: &mut TabStripOutcome,
) -> Option<TabDropZone> {
    let scroll_area_id = widget_ids::scroll_id(ui, "vertical_tab_list");
    let output = ui
        .scope(|ui| {
            ui.spacing_mut().scroll = tab_list_scroll_style();
            egui::ScrollArea::vertical()
                .id_salt(scroll_area_id)
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    collect_vertical_tab_entries(ui, app, duplicate_name_counts, outcome)
                })
        })
        .inner;
    maybe_auto_scroll_vertical_entries(ui, app, output.id, output.inner_rect, &output.state);
    let entries = output.inner;

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
        4.0,
        TabDropAxis::Vertical,
    );
    collect_slot_entries(ui, app, &context, outcome, |_, _| {})
}

fn maybe_auto_scroll_vertical_entries(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    scroll_area_id: egui::Id,
    viewport_rect: egui::Rect,
    scroll_state: &egui::scroll_area::State,
) {
    auto_scroll_vertical_tab_list(
        ui,
        scroll_area_id,
        viewport_rect,
        estimated_vertical_tab_list_height(app, 4.0),
        scroll_state,
    );
}

fn estimated_vertical_tab_list_height(app: &ScratchpadApp, spacing: f32) -> f32 {
    let tab_count = crate::app::app_state::workspace::display_tabs::total_tab_slots(app);
    if tab_count > 0 {
        (tab_count as f32 * crate::app::theme::TAB_HEIGHT)
            + ((tab_count.saturating_sub(1)) as f32 * spacing)
    } else {
        0.0
    }
}
