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
        TabDropAxis::Horizontal => {
            let marker_x = if drop_slot == 0 {
                first_rect.rect.left()
            } else if drop_slot > last_rect.index {
                last_rect.rect.right()
            } else {
                let mut previous_rect = first_rect.rect;
                let mut target_rect = first_rect.rect;
                for entry in &zone.entries {
                    if entry.index >= drop_slot {
                        target_rect = entry.rect;
                        break;
                    }
                    previous_rect = entry.rect;
                }
                (previous_rect.right() + target_rect.left()) * 0.5
            };

            let marker_rect = egui::Rect::from_center_size(
                egui::pos2(
                    marker_x,
                    (first_rect.rect.top() + last_rect.rect.bottom()) * 0.5,
                ),
                egui::vec2(3.0, TAB_HEIGHT - 6.0),
            );
            painter.rect_filled(marker_rect, 2.0, TAB_REORDER_MARKER_COLOR);
        }
        TabDropAxis::Vertical => {
            let marker_y = if drop_slot == 0 {
                first_rect.rect.top()
            } else if drop_slot > last_rect.index {
                last_rect.rect.bottom()
            } else {
                let mut previous_rect = first_rect.rect;
                let mut target_rect = first_rect.rect;
                for entry in &zone.entries {
                    if entry.index >= drop_slot {
                        target_rect = entry.rect;
                        break;
                    }
                    previous_rect = entry.rect;
                }
                (previous_rect.bottom() + target_rect.top()) * 0.5
            };

            let marker_width = first_rect.rect.width().max(24.0) - 8.0;
            let marker_rect = egui::Rect::from_center_size(
                egui::pos2(
                    (first_rect.rect.left() + last_rect.rect.right()) * 0.5,
                    marker_y,
                ),
                egui::vec2(marker_width, 3.0),
            );
            painter.rect_filled(marker_rect, 2.0, TAB_REORDER_MARKER_COLOR);
        }
    }
}

fn duplicate_name_counts(tabs: &[WorkspaceTab]) -> HashMap<String, usize> {
    let mut counts = HashMap::with_capacity(tabs.len());
    for tab in tabs {
        *counts.entry(tab.buffer.name.clone()).or_insert(0) += 1;
    }
    counts
}
