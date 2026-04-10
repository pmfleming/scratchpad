use super::{PaneBranch, PaneNode, SplitAxis};
use crate::app::domain::ViewId;

impl PaneNode {
    pub fn split_view(
        &mut self,
        target: ViewId,
        axis: SplitAxis,
        new_view_id: ViewId,
        new_view_first: bool,
        ratio: f32,
    ) -> bool {
        match self {
            Self::Leaf { view_id } if *view_id == target => {
                let clamped_ratio = ratio.clamp(0.2, 0.8);
                let existing_leaf = Box::new(Self::Leaf { view_id: *view_id });
                let new_leaf = Box::new(Self::Leaf {
                    view_id: new_view_id,
                });
                let (first, second) = if new_view_first {
                    (new_leaf, existing_leaf)
                } else {
                    (existing_leaf, new_leaf)
                };
                *self = Self::Split {
                    axis,
                    ratio: clamped_ratio,
                    first,
                    second,
                };
                true
            }
            Self::Leaf { .. } => false,
            Self::Split { first, second, .. } => {
                first.split_view(target, axis, new_view_id, new_view_first, ratio)
                    || second.split_view(target, axis, new_view_id, new_view_first, ratio)
            }
        }
    }

    pub fn split_view_with_node(
        &mut self,
        target: ViewId,
        axis: SplitAxis,
        new_node: PaneNode,
        new_view_first: bool,
        ratio: f32,
    ) -> bool {
        match self {
            Self::Leaf { view_id } if *view_id == target => {
                let clamped_ratio = ratio.clamp(0.2, 0.8);
                let existing_leaf = Box::new(Self::Leaf { view_id: *view_id });
                let new_node = Box::new(new_node);
                let (first, second) = if new_view_first {
                    (new_node, existing_leaf)
                } else {
                    (existing_leaf, new_node)
                };
                *self = Self::Split {
                    axis,
                    ratio: clamped_ratio,
                    first,
                    second,
                };
                true
            }
            Self::Leaf { .. } => false,
            Self::Split { first, second, .. } => {
                first.split_view_with_node(target, axis, new_node.clone(), new_view_first, ratio)
                    || second.split_view_with_node(target, axis, new_node, new_view_first, ratio)
            }
        }
    }

    pub fn resize_split(&mut self, path: &[PaneBranch], ratio: f32) -> bool {
        let clamped_ratio = ratio.clamp(0.2, 0.8);
        match path.split_first() {
            None => match self {
                Self::Split { ratio, .. } => {
                    *ratio = clamped_ratio;
                    true
                }
                Self::Leaf { .. } => false,
            },
            Some((PaneBranch::First, remainder)) => match self {
                Self::Split { first, .. } => first.resize_split(remainder, clamped_ratio),
                Self::Leaf { .. } => false,
            },
            Some((PaneBranch::Second, remainder)) => match self {
                Self::Split { second, .. } => second.resize_split(remainder, clamped_ratio),
                Self::Leaf { .. } => false,
            },
        }
    }

    pub fn balanced_from_view_ids(view_ids: &[ViewId], axis: SplitAxis) -> Option<Self> {
        match view_ids {
            [] => None,
            [view_id] => Some(Self::leaf(*view_id)),
            _ => {
                let first_count = view_ids.len().div_ceil(2);
                let second_count = view_ids.len() - first_count;
                let next_axis = match axis {
                    SplitAxis::Horizontal => SplitAxis::Vertical,
                    SplitAxis::Vertical => SplitAxis::Horizontal,
                };
                let first = Box::new(Self::balanced_from_view_ids(
                    &view_ids[..first_count],
                    next_axis,
                )?);
                let second = Box::new(Self::balanced_from_view_ids(
                    &view_ids[first_count..],
                    next_axis,
                )?);

                Some(Self::Split {
                    axis,
                    ratio: first_count as f32 / (first_count + second_count) as f32,
                    first,
                    second,
                })
            }
        }
    }
}
