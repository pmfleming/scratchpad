use super::state::{TabDragState, TabDropAxis, TabDropZone};
use crate::app::domain::WorkspaceTab;
use crate::app::theme::*;
use eframe::egui;
use std::collections::HashMap;

const TAB_REORDER_MARKER_COLOR: egui::Color32 = egui::Color32::from_rgb(104, 154, 232);

pub(super) fn paint_dragged_tab_ghost(
    ctx: &egui::Context,
    tabs: &[WorkspaceTab],
    drag_state: TabDragState,
) {
    let Some(tab) = tabs.get(drag_state.source_index) else {
        return;
    };

    let duplicate_name_counts = duplicate_name_counts(tabs);
    let has_duplicate = duplicate_name_counts
        .get(&tab.buffer.name)
        .copied()
        .unwrap_or(0)
        > 1;
    let display_name = tab.full_display_name(has_duplicate);
    let rect = egui::Rect::from_center_size(
        drag_state.current_pos,
        egui::vec2(TAB_BUTTON_WIDTH, TAB_HEIGHT),
    )
    .translate(egui::vec2(0.0, 2.0));
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("dragged_tab_ghost"),
    ));

    painter.rect_filled(rect, 4.0, TAB_ACTIVE_BG.gamma_multiply(0.92));
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, BORDER),
        egui::StrokeKind::Outside,
    );
    painter.text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        display_name,
        egui::FontId::proportional(14.0),
        TEXT_PRIMARY,
    );
}

pub(super) fn paint_tab_reorder_marker(ctx: &egui::Context, zone: &TabDropZone, drop_slot: usize) {
    let Some(first_rect) = zone.entries.first() else {
        return;
    };
    let Some(last_rect) = zone.entries.last() else {
        return;
    };

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("tab_reorder_marker"),
    ));

    match zone.axis {
        TabDropAxis::Horizontal => paint_horizontal_reorder_marker(
            &painter,
            zone,
            drop_slot,
            first_rect.rect,
            last_rect.rect,
            last_rect.index,
        ),
        TabDropAxis::Vertical => paint_vertical_reorder_marker(
            &painter,
            zone,
            drop_slot,
            first_rect.rect,
            last_rect.rect,
            last_rect.index,
        ),
    }
}

pub(super) fn paint_tab_combine_target(
    ctx: &egui::Context,
    zone: &TabDropZone,
    target_index: usize,
) {
    let Some(target_rect) = zone
        .entries
        .iter()
        .find(|entry| entry.index == target_index)
        .map(|entry| entry.rect)
    else {
        return;
    };

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("tab_combine_target"),
    ));
    painter.rect_filled(target_rect.shrink(2.0), 4.0, tab_combine_highlight_color());
    painter.rect_stroke(
        target_rect.shrink(2.0),
        4.0,
        egui::Stroke::new(1.0, TAB_REORDER_MARKER_COLOR),
        egui::StrokeKind::Inside,
    );
}

fn tab_combine_highlight_color() -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(104, 154, 232, 64)
}

fn paint_horizontal_reorder_marker(
    painter: &egui::Painter,
    zone: &TabDropZone,
    drop_slot: usize,
    first_rect: egui::Rect,
    last_rect: egui::Rect,
    last_index: usize,
) {
    let marker_x = horizontal_marker_position(zone, drop_slot, first_rect, last_rect, last_index);
    let marker_rect = egui::Rect::from_center_size(
        egui::pos2(marker_x, (first_rect.top() + last_rect.bottom()) * 0.5),
        egui::vec2(3.0, TAB_HEIGHT - 6.0),
    );
    painter.rect_filled(marker_rect, 2.0, TAB_REORDER_MARKER_COLOR);
}

fn paint_vertical_reorder_marker(
    painter: &egui::Painter,
    zone: &TabDropZone,
    drop_slot: usize,
    first_rect: egui::Rect,
    last_rect: egui::Rect,
    last_index: usize,
) {
    let marker_y = vertical_marker_position(zone, drop_slot, first_rect, last_rect, last_index);
    let marker_width = first_rect.width().max(24.0) - 8.0;
    let marker_rect = egui::Rect::from_center_size(
        egui::pos2((first_rect.left() + last_rect.right()) * 0.5, marker_y),
        egui::vec2(marker_width, 3.0),
    );
    painter.rect_filled(marker_rect, 2.0, TAB_REORDER_MARKER_COLOR);
}

fn horizontal_marker_position(
    zone: &TabDropZone,
    drop_slot: usize,
    first_rect: egui::Rect,
    last_rect: egui::Rect,
    last_index: usize,
) -> f32 {
    marker_position(
        zone,
        drop_slot,
        first_rect.left(),
        last_rect.right(),
        last_index,
        |previous, target| (previous.right() + target.left()) * 0.5,
    )
}

fn vertical_marker_position(
    zone: &TabDropZone,
    drop_slot: usize,
    first_rect: egui::Rect,
    last_rect: egui::Rect,
    last_index: usize,
) -> f32 {
    marker_position(
        zone,
        drop_slot,
        first_rect.top(),
        last_rect.bottom(),
        last_index,
        |previous, target| (previous.bottom() + target.top()) * 0.5,
    )
}

fn marker_position(
    zone: &TabDropZone,
    drop_slot: usize,
    first_edge: f32,
    last_edge: f32,
    last_index: usize,
    between: impl Fn(egui::Rect, egui::Rect) -> f32,
) -> f32 {
    if drop_slot == 0 {
        return first_edge;
    }
    if drop_slot > last_index {
        return last_edge;
    }

    let (previous_rect, target_rect) = surrounding_entry_rects(zone, drop_slot);
    between(previous_rect, target_rect)
}

fn surrounding_entry_rects(zone: &TabDropZone, drop_slot: usize) -> (egui::Rect, egui::Rect) {
    let Some(first_rect) = zone.entries.first().map(|entry| entry.rect) else {
        return (
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::ZERO),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::ZERO),
        );
    };
    let mut previous_rect = first_rect;
    let mut target_rect = first_rect;

    for entry in &zone.entries {
        if entry.index >= drop_slot {
            target_rect = entry.rect;
            break;
        }
        previous_rect = entry.rect;
    }

    (previous_rect, target_rect)
}

fn duplicate_name_counts(tabs: &[WorkspaceTab]) -> HashMap<String, usize> {
    let mut counts = HashMap::with_capacity(tabs.len());
    for tab in tabs {
        *counts.entry(tab.buffer.name.clone()).or_insert(0) += 1;
    }
    counts
}
