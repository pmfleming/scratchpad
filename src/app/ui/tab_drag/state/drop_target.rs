use super::{TabDropAxis, TabDropZone, TabRectEntry};
use eframe::egui;

pub(crate) enum TabDropIntent {
    Reorder {
        zone_index: usize,
        drop_slot: usize,
    },
    Combine {
        zone_index: usize,
        target_index: usize,
    },
}

pub(crate) fn tab_drop_slot(
    tab_rects: &[TabRectEntry],
    pointer_pos: egui::Pos2,
    axis: TabDropAxis,
) -> Option<usize> {
    let first_rect = tab_rects.first()?;
    let last_rect = tab_rects.last()?;
    let secondary_bounds = secondary_bounds(first_rect.rect, last_rect.rect, axis);
    let secondary_pointer = secondary_pointer(pointer_pos, axis);

    if !secondary_bounds.contains(&secondary_pointer) {
        return None;
    }

    let primary_pointer = primary_pointer(pointer_pos, axis);
    Some(find_drop_slot(tab_rects, primary_pointer, axis).unwrap_or(last_rect.index + 1))
}

pub(crate) fn locate_drop_intent(
    zones: &[TabDropZone],
    pointer_pos: egui::Pos2,
) -> Option<TabDropIntent> {
    zones.iter().enumerate().find_map(|(zone_index, zone)| {
        combine_target(&zone.entries, pointer_pos).map_or_else(
            || {
                tab_drop_slot(&zone.entries, pointer_pos, zone.axis).map(|drop_slot| {
                    TabDropIntent::Reorder {
                        zone_index,
                        drop_slot,
                    }
                })
            },
            |target_index| {
                Some(TabDropIntent::Combine {
                    zone_index,
                    target_index,
                })
            },
        )
    })
}

pub(crate) fn resolve_drop_slot(
    source_index: usize,
    drop_slot: usize,
    total_tab_count: usize,
) -> usize {
    let drop_slot = drop_slot.min(total_tab_count);
    let target_index = if drop_slot > source_index {
        drop_slot.saturating_sub(1)
    } else {
        drop_slot
    };

    target_index.min(total_tab_count.saturating_sub(1))
}

fn secondary_bounds(
    first_rect: egui::Rect,
    last_rect: egui::Rect,
    axis: TabDropAxis,
) -> std::ops::RangeInclusive<f32> {
    match axis {
        TabDropAxis::Horizontal => (first_rect.top() - 8.0)..=(last_rect.bottom() + 8.0),
        TabDropAxis::Vertical => (first_rect.left() - 8.0)..=(last_rect.right() + 8.0),
    }
}

fn secondary_pointer(pointer_pos: egui::Pos2, axis: TabDropAxis) -> f32 {
    match axis {
        TabDropAxis::Horizontal => pointer_pos.y,
        TabDropAxis::Vertical => pointer_pos.x,
    }
}

fn primary_pointer(pointer_pos: egui::Pos2, axis: TabDropAxis) -> f32 {
    match axis {
        TabDropAxis::Horizontal => pointer_pos.x,
        TabDropAxis::Vertical => pointer_pos.y,
    }
}

fn find_drop_slot(
    tab_rects: &[TabRectEntry],
    primary_pointer: f32,
    axis: TabDropAxis,
) -> Option<usize> {
    tab_rects
        .iter()
        .find(|entry| primary_pointer < entry_center(entry.rect, axis))
        .map(|entry| entry.index)
}

fn entry_center(rect: egui::Rect, axis: TabDropAxis) -> f32 {
    match axis {
        TabDropAxis::Horizontal => rect.center().x,
        TabDropAxis::Vertical => rect.center().y,
    }
}

fn combine_target(tab_rects: &[TabRectEntry], pointer_pos: egui::Pos2) -> Option<usize> {
    tab_rects.iter().find_map(|entry| {
        combine_rect(entry.rect)
            .contains(pointer_pos)
            .then_some(entry.index)
    })
}

fn combine_rect(rect: egui::Rect) -> egui::Rect {
    let shrink_x = (rect.width() * 0.22).clamp(10.0, 24.0);
    let shrink_y = (rect.height() * 0.18).clamp(3.0, 8.0);
    rect.shrink2(egui::vec2(shrink_x, shrink_y))
}
