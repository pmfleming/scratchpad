mod autoscroll;
mod drag;
mod drop_target;

use eframe::egui;

pub(super) const TAB_DRAG_THRESHOLD: f32 = 8.0;

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
    pub(crate) combine_enabled: bool,
}

pub(crate) use autoscroll::{auto_scroll_delta, vertical_auto_scroll_delta};
pub(crate) use drag::{
    active_drag_source_for_context, begin_tab_drag_if_needed, has_tab_drag_for_context,
    is_drag_active_for_context,
};
pub(crate) use drag::{
    clear_tab_drag_state, current_tab_drag_state_for_context, drag_is_active,
    update_current_tab_drag,
};
pub(crate) use drop_target::{TabDropIntent, locate_drop_intent, resolve_drop_slot};

#[cfg(test)]
mod tests {
    use super::{
        TabDropAxis, TabDropIntent, TabDropZone, TabRectEntry, auto_scroll_delta,
        locate_drop_intent, resolve_drop_slot, vertical_auto_scroll_delta,
    };
    use crate::app::ui::tab_drag::state::autoscroll::TAB_DRAG_AUTOSCROLL_MAX_STEP;
    use crate::app::ui::tab_drag::state::drop_target::tab_drop_slot;
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
                combine_enabled: true,
            },
            TabRectEntry {
                index: 1,
                rect: Rect::from_min_size(pos2(10.0, 44.0), vec2(140.0, 30.0)),
                combine_enabled: true,
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
                    combine_enabled: true,
                }],
            },
            TabDropZone {
                axis: TabDropAxis::Vertical,
                entries: vec![TabRectEntry {
                    index: 0,
                    rect: Rect::from_min_size(pos2(200.0, 50.0), vec2(140.0, 30.0)),
                    combine_enabled: true,
                }],
            },
        ];

        assert!(matches!(
            locate_drop_intent(&zones, pos2(240.0, 70.0), true),
            Some(TabDropIntent::Combine {
                zone_index: 1,
                target_index: 0,
            })
        ));
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
                        combine_enabled: true,
                    },
                    TabRectEntry {
                        index: 1,
                        rect: Rect::from_min_size(pos2(154.0, 10.0), vec2(140.0, 30.0)),
                        combine_enabled: true,
                    },
                ],
            },
            TabDropZone {
                axis: TabDropAxis::Vertical,
                entries: vec![
                    TabRectEntry {
                        index: 0,
                        rect: Rect::from_min_size(pos2(320.0, 50.0), vec2(140.0, 30.0)),
                        combine_enabled: true,
                    },
                    TabRectEntry {
                        index: 1,
                        rect: Rect::from_min_size(pos2(320.0, 84.0), vec2(140.0, 30.0)),
                        combine_enabled: true,
                    },
                    TabRectEntry {
                        index: 2,
                        rect: Rect::from_min_size(pos2(320.0, 118.0), vec2(140.0, 30.0)),
                        combine_enabled: true,
                    },
                ],
            },
        ];

        assert!(matches!(
            locate_drop_intent(&zones, pos2(360.0, 130.0), true),
            Some(TabDropIntent::Combine {
                zone_index: 1,
                target_index: 2,
            })
        ));
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

    #[test]
    fn vertical_auto_scroll_delta_pushes_up_near_top_edge() {
        let viewport = Rect::from_min_size(pos2(40.0, 10.0), vec2(140.0, 240.0));

        assert!(vertical_auto_scroll_delta(viewport, pos2(70.0, 12.0)) < 0.0);
    }

    #[test]
    fn vertical_auto_scroll_delta_pushes_down_near_bottom_edge() {
        let viewport = Rect::from_min_size(pos2(40.0, 10.0), vec2(140.0, 240.0));

        assert!(vertical_auto_scroll_delta(viewport, pos2(70.0, 248.0)) > 0.0);
    }
}
