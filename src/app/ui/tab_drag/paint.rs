use super::state::{TabDragState, TabDropAxis, TabDropZone, TabRectEntry};
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
    let Some(marker) = reorder_marker_rect(zone, drop_slot) else {
        return;
    };
    ctx.layer_painter(widget_ids::layer_id(
        egui::Order::Foreground,
        "tab_reorder_marker",
    ))
    .rect_filled(marker, 2.0, TAB_REORDER_MARKER_COLOR);
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

/// The thin bar drawn across the list where a dropped tab would land.
fn reorder_marker_rect(zone: &TabDropZone, drop_slot: usize) -> Option<egui::Rect> {
    let first = zone.entries.first()?.rect;
    let last = zone.entries.last()?;
    let past_end = drop_slot > last.index;
    let (previous, target) = surrounding_entry_rects(&zone.entries, drop_slot);
    let rect = match zone.axis {
        TabDropAxis::Horizontal => {
            let x = if drop_slot == 0 {
                first.left()
            } else if past_end {
                last.rect.right()
            } else {
                (previous.right() + target.left()) * 0.5
            };
            egui::Rect::from_center_size(
                egui::pos2(x, (first.top() + last.rect.bottom()) * 0.5),
                egui::vec2(3.0, (first.height() - 6.0).max(1.0)),
            )
        }
        TabDropAxis::Vertical => {
            let y = if drop_slot == 0 {
                first.top()
            } else if past_end {
                last.rect.bottom()
            } else {
                (previous.bottom() + target.top()) * 0.5
            };
            egui::Rect::from_center_size(
                egui::pos2((first.left() + last.rect.right()) * 0.5, y),
                egui::vec2(first.width().max(24.0) - 8.0, 3.0),
            )
        }
    };
    Some(rect)
}

/// The entries either side of `drop_slot` in a non-empty list.
fn surrounding_entry_rects(entries: &[TabRectEntry], drop_slot: usize) -> (egui::Rect, egui::Rect) {
    match entries.iter().position(|entry| entry.index >= drop_slot) {
        Some(target) => (entries[target.saturating_sub(1)].rect, entries[target].rect),
        None => (entries[entries.len() - 1].rect, entries[0].rect),
    }
}

#[cfg(test)]
mod tests {
    use super::{TabDropAxis, TabDropZone, reorder_marker_rect};
    use crate::app::ui::tab_drag::TabRectEntry;
    use eframe::egui;

    fn zone(axis: TabDropAxis, height: f32) -> TabDropZone {
        let step = match axis {
            TabDropAxis::Horizontal => egui::vec2(110.0, 0.0),
            TabDropAxis::Vertical => egui::vec2(0.0, height + 10.0),
        };
        TabDropZone {
            axis,
            entries: (0..3)
                .map(|index| TabRectEntry {
                    index,
                    rect: egui::Rect::from_min_size(
                        egui::pos2(20.0, 10.0) + step * index as f32,
                        egui::vec2(100.0, height),
                    ),
                    combine_enabled: true,
                })
                .collect(),
        }
    }

    #[test]
    fn reorder_marker_sits_at_list_ends_or_between_neighbours() {
        let empty = TabDropZone {
            axis: TabDropAxis::Horizontal,
            entries: Vec::new(),
        };
        assert_eq!(reorder_marker_rect(&empty, 0), None);
        for height in [30.0, 96.0] {
            let horizontal = zone(TabDropAxis::Horizontal, height);
            for (slot, x) in [(0, 20.0), (1, 125.0), (2, 235.0), (3, 340.0)] {
                let marker = reorder_marker_rect(&horizontal, slot).unwrap();
                assert_eq!(
                    marker.center(),
                    egui::pos2(x, 10.0 + height / 2.0),
                    "{slot}"
                );
                assert_eq!(marker.size(), egui::vec2(3.0, height - 6.0));
            }
            let vertical = zone(TabDropAxis::Vertical, height);
            let ys = [
                10.0,
                15.0 + height,
                25.0 + 2.0 * height,
                30.0 + 3.0 * height,
            ];
            for (slot, y) in ys.into_iter().enumerate() {
                let marker = reorder_marker_rect(&vertical, slot).unwrap();
                assert_eq!(marker.center(), egui::pos2(70.0, y), "{slot}");
                assert_eq!(marker.size(), egui::vec2(92.0, 3.0));
            }
        }
    }
}
