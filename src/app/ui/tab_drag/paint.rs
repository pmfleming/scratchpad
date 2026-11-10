use super::state::{TabDragState, TabDropAxis, TabDropZone};
use crate::app::app_state::{ScratchpadApp, workspace::display_tabs};
use crate::app::theme::{border_for_visuals, tab_active_bg_for_visuals, text_primary_for_visuals};
use crate::app::ui::widget_ids;
use eframe::egui;
use std::collections::HashMap;

const TAB_REORDER_MARKER_COLOR: egui::Color32 = egui::Color32::from_rgb(104, 154, 232);

pub(super) fn paint_dragged_tab_ghost(
    ctx: &egui::Context,
    app: &ScratchpadApp,
    drag_state: TabDragState,
    duplicate_name_counts: &HashMap<String, usize>,
) {
    let dragged_slots = drag_state.dragged_indices.as_slice();
    let Some(first_label) = display_tabs::display_tab_name_at_slot_with_counts(
        app,
        drag_state.source_index,
        duplicate_name_counts,
    ) else {
        return;
    };
    let display_name = if dragged_slots.len() > 1 {
        format!("[{} tabs] {}", dragged_slots.len(), first_label)
    } else {
        first_label
    };
    let rect = egui::Rect::from_center_size(
        drag_state.current_pos,
        egui::vec2(
            crate::app::chrome::tab_button_width(app.state.app_settings.font_size()),
            crate::app::chrome::tab_row_height(ctx, app.state.app_settings.font_size()),
        ),
    )
    .translate(egui::vec2(0.0, 2.0));
    let painter = ctx.layer_painter(widget_ids::layer_id(
        egui::Order::Foreground,
        "dragged_tab_ghost",
    ));
    let visuals = &ctx.global_style().visuals;

    if dragged_slots.len() > 1 {
        for layer in [2.0_f32, 1.0_f32] {
            let shadow_rect = rect.translate(egui::vec2(layer * 6.0, layer * 4.0));
            painter.rect_filled(
                shadow_rect,
                4.0,
                tab_active_bg_for_visuals(visuals).gamma_multiply(0.35),
            );
            painter.rect_stroke(
                shadow_rect,
                4.0,
                egui::Stroke::new(1.0, border_for_visuals(visuals).gamma_multiply(0.4)),
                egui::StrokeKind::Outside,
            );
        }
    }

    painter.rect_filled(
        rect,
        4.0,
        tab_active_bg_for_visuals(visuals).gamma_multiply(0.92),
    );
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, border_for_visuals(visuals)),
        egui::StrokeKind::Outside,
    );
    painter.with_clip_rect(rect).text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        display_name,
        crate::app::chrome::tab_label_font_id(app.state.app_settings.font_size()),
        text_primary_for_visuals(visuals),
    );

    if dragged_slots.len() > 1 {
        let badge_rect = egui::Rect::from_center_size(
            rect.right_top() + egui::vec2(-18.0, 12.0),
            egui::vec2(28.0, 18.0),
        );
        painter.rect_filled(badge_rect, 9.0, TAB_REORDER_MARKER_COLOR);
        painter.text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            dragged_slots.len().to_string(),
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }
}

pub(super) fn paint_tab_reorder_marker(ctx: &egui::Context, zone: &TabDropZone, drop_slot: usize) {
    let Some(first_rect) = zone.entries.first() else {
        return;
    };
    let Some(last_rect) = zone.entries.last() else {
        return;
    };

    let painter = ctx.layer_painter(widget_ids::layer_id(
        egui::Order::Foreground,
        "tab_reorder_marker",
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

    paint_combine_target(ctx, target_rect);
}

pub(super) fn paint_combine_target(ctx: &egui::Context, target_rect: egui::Rect) {
    let painter = ctx.layer_painter(widget_ids::layer_id(
        egui::Order::Foreground,
        "tab_combine_target",
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
    let marker_x = reorder_marker_position(zone, drop_slot, first_rect, last_rect, last_index);
    let marker_rect = egui::Rect::from_center_size(
        egui::pos2(marker_x, (first_rect.top() + last_rect.bottom()) * 0.5),
        egui::vec2(3.0, (first_rect.height() - 6.0).max(1.0)),
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
    let marker_y = reorder_marker_position(zone, drop_slot, first_rect, last_rect, last_index);
    let marker_width = first_rect.width().max(24.0) - 8.0;
    let marker_rect = egui::Rect::from_center_size(
        egui::pos2((first_rect.left() + last_rect.right()) * 0.5, marker_y),
        egui::vec2(marker_width, 3.0),
    );
    painter.rect_filled(marker_rect, 2.0, TAB_REORDER_MARKER_COLOR);
}

fn reorder_marker_position(
    zone: &TabDropZone,
    drop_slot: usize,
    first_rect: egui::Rect,
    last_rect: egui::Rect,
    last_index: usize,
) -> f32 {
    match zone.axis {
        TabDropAxis::Horizontal => marker_position(
            zone,
            drop_slot,
            first_rect.left(),
            last_rect.right(),
            last_index,
            |previous, target| (previous.right() + target.left()) * 0.5,
        ),
        TabDropAxis::Vertical => marker_position(
            zone,
            drop_slot,
            first_rect.top(),
            last_rect.bottom(),
            last_index,
            |previous, target| (previous.bottom() + target.top()) * 0.5,
        ),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_reorder_marker_spans_the_actual_tab_height() {
        for height in [30.0, 96.0] {
            let ctx = egui::Context::default();
            let rect = egui::Rect::from_min_size(egui::pos2(20.0, 10.0), egui::vec2(400.0, height));
            let zone = TabDropZone {
                axis: TabDropAxis::Horizontal,
                entries: vec![super::super::TabRectEntry {
                    index: 0,
                    rect,
                    combine_enabled: true,
                }],
            };
            let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
                paint_tab_reorder_marker(&ctx, &zone, 0);
            });
            output.textures_delta.clear();
            let marker = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(shape) if shape.fill == TAB_REORDER_MARKER_COLOR => {
                        Some(shape.rect)
                    }
                    _ => None,
                })
                .expect("Reorder marker");
            assert_eq!(marker.height(), height - 6.0);
            assert_eq!(marker.center().y, rect.center().y);
        }
    }
}
