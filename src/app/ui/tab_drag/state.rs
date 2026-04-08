use eframe::egui;

pub(super) const TAB_DRAG_THRESHOLD: f32 = 8.0;
const TAB_DRAG_AUTOSCROLL_EDGE: f32 = 36.0;
const TAB_DRAG_AUTOSCROLL_MAX_STEP: f32 = 18.0;
const TAB_DRAG_VERTICAL_MARGIN: f32 = 12.0;

#[derive(Clone, Copy)]
pub(super) struct TabDragState {
    pub(super) source_index: usize,
    pub(super) start_pos: egui::Pos2,
    pub(super) current_pos: egui::Pos2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabDropAxis {
    Horizontal,
    Vertical,
}

pub(crate) struct TabDropZone {
    pub(crate) axis: TabDropAxis,
    pub(crate) entries: Vec<TabRectEntry>,
}

#[derive(Clone, Copy)]
pub(crate) struct TabRectEntry {
    pub(crate) index: usize,
    pub(crate) rect: egui::Rect,
}

pub(crate) fn begin_tab_drag_if_needed(
    ui: &egui::Ui,
    index: usize,
    tab_response: &egui::Response,
    close_response: &egui::Response,
) {
    if current_tab_drag_state(ui).is_some() || close_response.hovered() {
        return;
    }

    if !tab_response.hovered() || !ui.input(|input| input.pointer.primary_pressed()) {
        return;
    }

    let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };

    ui.ctx().data_mut(|data| {
        data.insert_temp(
            tab_drag_state_id(),
            TabDragState {
                source_index: index,
                start_pos: pointer_pos,
                current_pos: pointer_pos,
            },
        );
    });
}

pub(crate) fn active_drag_source_for_context(ctx: &egui::Context) -> Option<usize> {
    let drag_state = current_tab_drag_state_for_context(ctx)?;
    drag_is_active(drag_state).then_some(drag_state.source_index)
}

pub(crate) fn is_drag_active_for_context(ctx: &egui::Context) -> bool {
    current_tab_drag_state_for_context(ctx).is_some_and(drag_is_active)
}

pub(super) fn update_current_tab_drag(ui: &egui::Ui) -> Option<TabDragState> {
    let mut drag_state = current_tab_drag_state(ui)?;

    if let Some(pointer_pos) = ui.input(|input| input.pointer.latest_pos()) {
        drag_state.current_pos = pointer_pos;
        ui.ctx().data_mut(|data| {
            data.insert_temp(tab_drag_state_id(), drag_state);
        });
    }

    Some(drag_state)
}

pub(super) fn current_tab_drag_state_for_context(ctx: &egui::Context) -> Option<TabDragState> {
    ctx.data(|data| data.get_temp::<TabDragState>(tab_drag_state_id()))
}

pub(super) fn drag_is_active(drag_state: TabDragState) -> bool {
    drag_state.start_pos.distance(drag_state.current_pos) >= TAB_DRAG_THRESHOLD
}

pub(super) fn tab_drop_slot(
    tab_rects: &[TabRectEntry],
    pointer_pos: egui::Pos2,
    axis: TabDropAxis,
) -> Option<usize> {
    let first_rect = tab_rects.first()?;
    let last_rect = tab_rects.last()?;
    let secondary_bounds = match axis {
        TabDropAxis::Horizontal => (first_rect.rect.top() - 8.0)..=(last_rect.rect.bottom() + 8.0),
        TabDropAxis::Vertical => (first_rect.rect.left() - 8.0)..=(last_rect.rect.right() + 8.0),
    };

    let secondary_pointer = match axis {
        TabDropAxis::Horizontal => pointer_pos.y,
        TabDropAxis::Vertical => pointer_pos.x,
    };

    if !secondary_bounds.contains(&secondary_pointer) {
        return None;
    }

    for entry in tab_rects {
        let primary_pointer = match axis {
            TabDropAxis::Horizontal => pointer_pos.x,
            TabDropAxis::Vertical => pointer_pos.y,
        };
        let entry_center = match axis {
            TabDropAxis::Horizontal => entry.rect.center().x,
            TabDropAxis::Vertical => entry.rect.center().y,
        };

        if primary_pointer < entry_center {
            return Some(entry.index);
        }
    }

    Some(last_rect.index + 1)
}

pub(super) fn locate_drop_slot(
    zones: &[TabDropZone],
    pointer_pos: egui::Pos2,
) -> Option<(usize, usize)> {
    zones.iter().enumerate().find_map(|(zone_index, zone)| {
        tab_drop_slot(&zone.entries, pointer_pos, zone.axis)
            .map(|drop_slot| (zone_index, drop_slot))
    })
}

pub(super) fn resolve_drop_slot(
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

pub(super) fn clear_tab_drag_state(ui: &egui::Ui) {
    ui.ctx().data_mut(|data| {
        data.remove::<TabDragState>(tab_drag_state_id());
    });
}

pub(super) fn auto_scroll_delta(viewport_rect: egui::Rect, pointer_pos: egui::Pos2) -> f32 {
    let vertical_bounds = (viewport_rect.top() - TAB_DRAG_VERTICAL_MARGIN)
        ..=(viewport_rect.bottom() + TAB_DRAG_VERTICAL_MARGIN);
    if !vertical_bounds.contains(&pointer_pos.y) {
        return 0.0;
    }

    let left_distance = pointer_pos.x - viewport_rect.left();
    if left_distance <= TAB_DRAG_AUTOSCROLL_EDGE {
        let intensity = (1.0 - left_distance / TAB_DRAG_AUTOSCROLL_EDGE).clamp(0.0, 1.0);
        return -TAB_DRAG_AUTOSCROLL_MAX_STEP * intensity;
    }

    let right_distance = viewport_rect.right() - pointer_pos.x;
    if right_distance <= TAB_DRAG_AUTOSCROLL_EDGE {
        let intensity = (1.0 - right_distance / TAB_DRAG_AUTOSCROLL_EDGE).clamp(0.0, 1.0);
        return TAB_DRAG_AUTOSCROLL_MAX_STEP * intensity;
    }

    0.0
}

fn tab_drag_state_id() -> egui::Id {
    egui::Id::new("tab_strip_drag_state")
}

fn current_tab_drag_state(ui: &egui::Ui) -> Option<TabDragState> {
    ui.ctx()
        .data(|data| data.get_temp::<TabDragState>(tab_drag_state_id()))
}

#[cfg(test)]
mod tests {
    use super::{
        TAB_DRAG_AUTOSCROLL_MAX_STEP, TabDropAxis, TabDropZone, TabRectEntry, auto_scroll_delta,
        locate_drop_slot, resolve_drop_slot, tab_drop_slot,
    };
    use eframe::egui::{Rect, pos2, vec2};

    #[test]
    fn resolve_drop_slot_allows_drag_to_far_right_end() {
        assert_eq!(resolve_drop_slot(1, 5, 5), 4);
    }

    #[test]
    fn resolve_drop_slot_keeps_leftward_moves_stable() {
        assert_eq!(resolve_drop_slot(3, 1, 5), 1);
    }

    #[test]
    fn resolve_drop_slot_adjusts_for_mid_stream_right_move() {
        assert_eq!(resolve_drop_slot(1, 3, 5), 2);
    }

    #[test]
    fn tab_drop_slot_supports_vertical_lists() {
        let entries = vec![
            TabRectEntry {
                index: 0,
                rect: Rect::from_min_size(pos2(10.0, 10.0), vec2(140.0, 30.0)),
            },
            TabRectEntry {
                index: 1,
                rect: Rect::from_min_size(pos2(10.0, 44.0), vec2(140.0, 30.0)),
            },
        ];

        assert_eq!(
            tab_drop_slot(&entries, pos2(70.0, 20.0), TabDropAxis::Vertical),
            Some(0)
        );
        assert_eq!(
            tab_drop_slot(&entries, pos2(70.0, 58.0), TabDropAxis::Vertical),
            Some(1)
        );
        assert_eq!(
            tab_drop_slot(&entries, pos2(70.0, 90.0), TabDropAxis::Vertical),
            Some(2)
        );
    }

    #[test]
    fn locate_drop_slot_picks_matching_zone() {
        let zones = vec![
            TabDropZone {
                axis: TabDropAxis::Horizontal,
                entries: vec![TabRectEntry {
                    index: 0,
                    rect: Rect::from_min_size(pos2(10.0, 10.0), vec2(140.0, 30.0)),
                }],
            },
            TabDropZone {
                axis: TabDropAxis::Vertical,
                entries: vec![TabRectEntry {
                    index: 0,
                    rect: Rect::from_min_size(pos2(200.0, 50.0), vec2(140.0, 30.0)),
                }],
            },
        ];

        assert_eq!(locate_drop_slot(&zones, pos2(220.0, 70.0)), Some((1, 1)));
    }

    #[test]
    fn locate_drop_slot_supports_shared_tab_bar_and_overflow_zones() {
        let zones = vec![
            TabDropZone {
                axis: TabDropAxis::Horizontal,
                entries: vec![
                    TabRectEntry {
                        index: 0,
                        rect: Rect::from_min_size(pos2(10.0, 10.0), vec2(140.0, 30.0)),
                    },
                    TabRectEntry {
                        index: 1,
                        rect: Rect::from_min_size(pos2(154.0, 10.0), vec2(140.0, 30.0)),
                    },
                ],
            },
            TabDropZone {
                axis: TabDropAxis::Vertical,
                entries: vec![
                    TabRectEntry {
                        index: 0,
                        rect: Rect::from_min_size(pos2(320.0, 50.0), vec2(140.0, 30.0)),
                    },
                    TabRectEntry {
                        index: 1,
                        rect: Rect::from_min_size(pos2(320.0, 84.0), vec2(140.0, 30.0)),
                    },
                    TabRectEntry {
                        index: 2,
                        rect: Rect::from_min_size(pos2(320.0, 118.0), vec2(140.0, 30.0)),
                    },
                ],
            },
        ];

        assert_eq!(locate_drop_slot(&zones, pos2(360.0, 130.0)), Some((1, 2)));
    }

    #[test]
    fn auto_scroll_delta_pushes_left_near_left_edge() {
        let viewport = Rect::from_min_size(pos2(40.0, 10.0), vec2(240.0, 30.0));

        assert!(auto_scroll_delta(viewport, pos2(42.0, 24.0)) < 0.0);
    }

    #[test]
    fn auto_scroll_delta_pushes_right_near_right_edge() {
        let viewport = Rect::from_min_size(pos2(40.0, 10.0), vec2(240.0, 30.0));

        assert!(auto_scroll_delta(viewport, pos2(278.0, 24.0)) > 0.0);
    }

    #[test]
    fn auto_scroll_delta_is_zero_outside_hot_zone() {
        let viewport = Rect::from_min_size(pos2(40.0, 10.0), vec2(240.0, 30.0));

        assert_eq!(auto_scroll_delta(viewport, pos2(160.0, 24.0)), 0.0);
    }

    #[test]
    fn auto_scroll_delta_caps_at_max_step() {
        let viewport = Rect::from_min_size(pos2(40.0, 10.0), vec2(240.0, 30.0));

        assert_eq!(
            auto_scroll_delta(viewport, pos2(viewport.left(), 24.0)),
            -TAB_DRAG_AUTOSCROLL_MAX_STEP
        );
        assert_eq!(
            auto_scroll_delta(viewport, pos2(viewport.right(), 24.0)),
            TAB_DRAG_AUTOSCROLL_MAX_STEP
        );
    }
}
