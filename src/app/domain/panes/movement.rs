use super::split::clamp_split_ratio;
use super::{PaneNode, SplitAxis, TileDirection};
use crate::app::domain::ViewId;

const MOVED_TILE_SPLIT_RATIO: f32 = 0.5;
const FOCAL_POINT_OFFSET: f32 = 0.000_1;

#[derive(Clone, Copy)]
struct PaneRect {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl PaneRect {
    const WORKSPACE: Self = Self {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 1.0,
        max_y: 1.0,
    };

    fn split(self, axis: SplitAxis, ratio: f32) -> (Self, Self) {
        match axis {
            SplitAxis::Vertical => {
                let split_x = self.min_x + (self.max_x - self.min_x) * ratio;
                (
                    Self {
                        max_x: split_x,
                        ..self
                    },
                    Self {
                        min_x: split_x,
                        ..self
                    },
                )
            }
            SplitAxis::Horizontal => {
                let split_y = self.min_y + (self.max_y - self.min_y) * ratio;
                (
                    Self {
                        max_y: split_y,
                        ..self
                    },
                    Self {
                        min_y: split_y,
                        ..self
                    },
                )
            }
        }
    }

    fn focal_point(self, direction: TileDirection) -> (f32, f32) {
        let center_x = (self.min_x + self.max_x) * 0.5;
        let center_y = (self.min_y + self.max_y) * 0.5;
        match direction {
            TileDirection::Left => (self.min_x - FOCAL_POINT_OFFSET, center_y),
            TileDirection::Right => (self.max_x + FOCAL_POINT_OFFSET, center_y),
            TileDirection::Up => (center_x, self.min_y - FOCAL_POINT_OFFSET),
            TileDirection::Down => (center_x, self.max_y + FOCAL_POINT_OFFSET),
        }
    }

    fn distance_squared_to(self, point: (f32, f32)) -> f32 {
        let dx = if point.0 < self.min_x {
            self.min_x - point.0
        } else if point.0 > self.max_x {
            point.0 - self.max_x
        } else {
            0.0
        };
        let dy = if point.1 < self.min_y {
            self.min_y - point.1
        } else if point.1 > self.max_y {
            point.1 - self.max_y
        } else {
            0.0
        };
        dx * dx + dy * dy
    }
}

impl PaneNode {
    /// Moves a leaf using Hyprland dwindle's focal-point remove/reinsert model.
    pub fn move_view_in_direction(&mut self, view_id: ViewId, direction: TileDirection) -> bool {
        if self.leaf_count() <= 1 {
            return false;
        }

        let Some(active_rect) = self.rect_for_view(view_id, PaneRect::WORKSPACE) else {
            return false;
        };
        let focal_point = active_rect.focal_point(direction);
        let original_tree = self.clone();

        if !self.remove_view(view_id) {
            return false;
        }

        let Some((target_view_id, target_rect)) =
            self.closest_view_to_point(focal_point, PaneRect::WORKSPACE)
        else {
            *self = original_tree;
            return false;
        };
        let (axis, moved_view_first) = smart_split_placement(target_rect, focal_point);

        if self.split_view(
            target_view_id,
            axis,
            view_id,
            moved_view_first,
            MOVED_TILE_SPLIT_RATIO,
        ) {
            true
        } else {
            *self = original_tree;
            false
        }
    }

    fn rect_for_view(&self, target: ViewId, rect: PaneRect) -> Option<PaneRect> {
        match self {
            Self::Leaf { view_id } => (*view_id == target).then_some(rect),
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (first_rect, second_rect) = rect.split(*axis, clamp_split_ratio(*ratio));
                first
                    .rect_for_view(target, first_rect)
                    .or_else(|| second.rect_for_view(target, second_rect))
            }
        }
    }

    fn closest_view_to_point(
        &self,
        point: (f32, f32),
        rect: PaneRect,
    ) -> Option<(ViewId, PaneRect)> {
        let mut closest = None;
        self.find_closest_view(point, rect, &mut closest);
        closest.map(|(view_id, rect, _)| (view_id, rect))
    }

    fn find_closest_view(
        &self,
        point: (f32, f32),
        rect: PaneRect,
        closest: &mut Option<(ViewId, PaneRect, f32)>,
    ) {
        match self {
            Self::Leaf { view_id } => {
                let distance = rect.distance_squared_to(point);
                if closest.is_none_or(|(_, _, closest_distance)| distance < closest_distance) {
                    *closest = Some((*view_id, rect, distance));
                }
            }
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (first_rect, second_rect) = rect.split(*axis, clamp_split_ratio(*ratio));
                first.find_closest_view(point, first_rect, closest);
                second.find_closest_view(point, second_rect, closest);
            }
        }
    }
}

fn smart_split_placement(target: PaneRect, focal_point: (f32, f32)) -> (SplitAxis, bool) {
    let center_x = (target.min_x + target.max_x) * 0.5;
    let center_y = (target.min_y + target.max_y) * 0.5;
    let delta_x = focal_point.0 - center_x;
    let delta_y = focal_point.1 - center_y;
    let target_aspect = (target.max_y - target.min_y) / (target.max_x - target.min_x);
    let focal_slope = (delta_y / delta_x).abs();

    if focal_slope < target_aspect {
        (SplitAxis::Vertical, delta_x <= 0.0)
    } else {
        (SplitAxis::Horizontal, delta_y <= 0.0)
    }
}
