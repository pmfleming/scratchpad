use super::shared::{collect_slot_entries, slot_cell_context};
use super::{DuplicateNameCounts, apply_tab_drag_feedback};
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::theme::{BUTTON_SIZE, TAB_BUTTON_WIDTH, TAB_HEIGHT, action_bg, action_hover_bg};
use crate::app::ui::tab_drag::{self, TabDropAxis, TabDropZone, TabRectEntry};
use crate::app::ui::tab_overflow;
use crate::app::ui::tab_strip::{
    HeaderLayout, TabStripOutcome, maybe_auto_scroll_tab_strip, record_visible_tab,
};
use eframe::egui::{self, Sense};
use std::collections::HashSet;

struct TabStripEntriesContext<'a> {
    app: &'a mut ScratchpadApp,
    duplicate_name_counts: &'a DuplicateNameCounts,
    viewport_rect: egui::Rect,
    visible_tab_indices: &'a mut HashSet<usize>,
    outcome: &'a mut TabStripOutcome,
}

pub(super) fn show_tab_region(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    duplicate_name_counts: &DuplicateNameCounts,
) -> TabStripOutcome {
    let mut visible_tab_indices = HashSet::new();
    let mut outcome = TabStripOutcome::default();

    ui.allocate_ui_with_layout(
        egui::vec2(layout.tab_area_width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            tab_drag::sync_drag_state(ui);
            ui.spacing_mut().item_spacing.x = 0.0;
            let drop_zones = collect_tab_drop_zones(
                ctx,
                ui,
                app,
                layout,
                duplicate_name_counts,
                &mut visible_tab_indices,
                &mut outcome,
            );
            apply_tab_drag_feedback(ui, app, &drop_zones, &mut outcome);
            render_new_tab_action(ui, app, layout.spacing);
            show_drag_region(ctx, ui, layout.drag_width);
        },
    );

    outcome
}

fn allocate_tab_strip_entries(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    scroll_area_id: egui::Id,
    duplicate_name_counts: &DuplicateNameCounts,
    visible_tab_indices: &mut HashSet<usize>,
    outcome: &mut TabStripOutcome,
) -> Vec<TabRectEntry> {
    ui.allocate_ui_with_layout(
        egui::vec2(layout.visible_strip_width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            configure_tab_strip_viewport(ui, layout.visible_strip_width);
            let viewport_rect = ui.max_rect();
            maybe_auto_scroll_tab_strip(ui, app, layout, scroll_area_id, viewport_rect);
            let mut context = TabStripEntriesContext {
                app,
                duplicate_name_counts,
                viewport_rect,
                visible_tab_indices,
                outcome,
            };
            render_tab_strip_entries(ui, layout, scroll_area_id, &mut context)
        },
    )
    .inner
}

fn configure_tab_strip_viewport(ui: &mut egui::Ui, visible_strip_width: f32) {
    ui.set_width(visible_strip_width);
    ui.set_min_width(visible_strip_width);
    ui.set_max_width(visible_strip_width);
}

fn render_tab_strip_entries(
    ui: &mut egui::Ui,
    layout: &HeaderLayout,
    scroll_area_id: egui::Id,
    context: &mut TabStripEntriesContext<'_>,
) -> Vec<TabRectEntry> {
    egui::ScrollArea::horizontal()
        .id_salt(scroll_area_id)
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = layout.spacing;
            ui.horizontal(|ui| collect_tab_entries(ui, context)).inner
        })
        .inner
}

fn collect_tab_entries(
    ui: &mut egui::Ui,
    context: &mut TabStripEntriesContext<'_>,
) -> Vec<TabRectEntry> {
    let cell_context =
        slot_cell_context(context.app, context.duplicate_name_counts, TAB_BUTTON_WIDTH);
    collect_slot_entries(
        ui,
        context.app,
        &cell_context,
        context.outcome,
        |slot_index, rect| {
            record_visible_tab(
                slot_index,
                rect,
                context.viewport_rect,
                context.visible_tab_indices,
            );
        },
    )
}

fn collect_tab_drop_zones(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    duplicate_name_counts: &DuplicateNameCounts,
    visible_tab_indices: &mut HashSet<usize>,
    outcome: &mut TabStripOutcome,
) -> Vec<TabDropZone> {
    let mut drop_zones = Vec::new();

    if layout.visible_strip_width > 0.0
        && let Some(tab_bar_zone) = show_scrolling_tab_strip(
            ui,
            app,
            layout,
            duplicate_name_counts,
            visible_tab_indices,
            outcome,
        )
    {
        drop_zones.push(tab_bar_zone);
    }

    if (layout.has_overflow || app.overflow_popup_open)
        && let Some(overflow_zone) = show_overflow_controls(
            ctx,
            ui,
            app,
            layout,
            visible_tab_indices,
            duplicate_name_counts,
            outcome,
        )
    {
        drop_zones.push(overflow_zone);
    }

    drop_zones
}

fn show_scrolling_tab_strip(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    duplicate_name_counts: &DuplicateNameCounts,
    visible_tab_indices: &mut HashSet<usize>,
    outcome: &mut TabStripOutcome,
) -> Option<TabDropZone> {
    let scroll_area_id = ui.id().with("tab_strip");
    let entries = allocate_tab_strip_entries(
        ui,
        app,
        layout,
        scroll_area_id,
        duplicate_name_counts,
        visible_tab_indices,
        outcome,
    );

    (!entries.is_empty()).then_some(TabDropZone {
        axis: TabDropAxis::Horizontal,
        entries,
    })
}

fn render_new_tab_action(ui: &mut egui::Ui, app: &mut ScratchpadApp, spacing: f32) {
    ui.add_space(spacing);
    if crate::app::chrome::phosphor_button(
        ui,
        egui_phosphor::regular::PLUS,
        BUTTON_SIZE,
        action_bg(ui),
        action_hover_bg(ui),
        "New Tab",
    )
    .clicked()
    {
        app.handle_command(AppCommand::NewTab);
    }
}

fn show_overflow_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    visible_tab_indices: &HashSet<usize>,
    duplicate_name_counts: &DuplicateNameCounts,
    outcome: &mut TabStripOutcome,
) -> Option<TabDropZone> {
    ui.add_space(layout.spacing);
    let mut overflow_popup_open = app.overflow_popup_open;
    let overflow_outcome = tab_overflow::show_overflow_button(
        ctx,
        ui,
        app,
        &mut overflow_popup_open,
        visible_tab_indices,
        duplicate_name_counts,
    );
    app.overflow_popup_open = overflow_popup_open;

    outcome.activated_tab = outcome.activated_tab.or(overflow_outcome.activated_tab);
    outcome.activate_settings = outcome.activate_settings || overflow_outcome.activate_settings;
    outcome.promote_all_files_tab = outcome
        .promote_all_files_tab
        .or(overflow_outcome.promote_all_files_tab);
    outcome.close_requested_tab = outcome
        .close_requested_tab
        .or(overflow_outcome.close_requested_tab);
    outcome.close_settings = outcome.close_settings || overflow_outcome.close_settings;
    overflow_outcome.drop_zone
}

fn show_drag_region(ctx: &egui::Context, ui: &mut egui::Ui, drag_width: f32) {
    if drag_width <= 0.0 {
        return;
    }

    let (rect, drag_response) =
        ui.allocate_exact_size(egui::vec2(drag_width, TAB_HEIGHT), Sense::click_and_drag());
    if drag_response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if drag_response.double_clicked() {
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    ui.painter()
        .rect_filled(rect, 0.0, crate::app::theme::header_bg(ui));
}
