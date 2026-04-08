use crate::app::domain::ViewId;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaneBranch {
    First,
    Second,
}

pub type SplitPath = Vec<PaneBranch>;

#[derive(Clone)]
pub enum PaneNode {
    Leaf {
        view_id: ViewId,
    },
    Split {
        axis: SplitAxis,
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

impl PaneNode {
    pub fn leaf(view_id: ViewId) -> Self {
        Self::Leaf { view_id }
    }

    pub fn leaf_count(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
        }
    }

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

    pub fn remove_view(&mut self, target: ViewId) -> bool {
        match self {
            Self::Leaf { .. } => false,
            Self::Split { first, second, .. } => {
                if matches!(first.as_ref(), Self::Leaf { view_id } if *view_id == target) {
                    *self = (**second).clone();
                    return true;
                }
                if matches!(second.as_ref(), Self::Leaf { view_id } if *view_id == target) {
                    *self = (**first).clone();
                    return true;
                }
                first.remove_view(target) || second.remove_view(target)
            }
        }
    }

    pub fn retain_views(&mut self, valid_view_ids: &HashSet<ViewId>) -> bool {
        match self {
            Self::Leaf { view_id } => valid_view_ids.contains(view_id),
            Self::Split { first, second, .. } => {
                let first_valid = first.retain_views(valid_view_ids);
                let second_valid = second.retain_views(valid_view_ids);

                match (first_valid, second_valid) {
                    (true, true) => true,
                    (true, false) => {
                        *self = (**first).clone();
                        true
                    }
                    (false, true) => {
                        *self = (**second).clone();
                        true
                    }
                    (false, false) => false,
                }
            }
        }
    }

    pub fn collect_view_ids(&self, output: &mut HashSet<ViewId>) {
        match self {
            Self::Leaf { view_id } => {
                output.insert(*view_id);
            }
            Self::Split { first, second, .. } => {
                first.collect_view_ids(output);
                second.collect_view_ids(output);
            }
        }
    }

    pub fn collect_view_ids_in_order(&self, output: &mut Vec<ViewId>) {
        match self {
            Self::Leaf { view_id } => output.push(*view_id),
            Self::Split { first, second, .. } => {
                first.collect_view_ids_in_order(output);
                second.collect_view_ids_in_order(output);
            }
        }
    }

    pub fn contains_view(&self, target: ViewId) -> bool {
        match self {
            Self::Leaf { view_id } => *view_id == target,
            Self::Split { first, second, .. } => {
                first.contains_view(target) || second.contains_view(target)
            }
        }
    }

    pub fn first_view_id(&self) -> ViewId {
        match self {
            Self::Leaf { view_id } => *view_id,
            Self::Split { first, .. } => first.first_view_id(),
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

    pub fn shallowest_leaf(&self) -> (ViewId, usize) {
        self.shallowest_leaf_at_depth(0)
    }

    fn shallowest_leaf_at_depth(&self, depth: usize) -> (ViewId, usize) {
        match self {
            Self::Leaf { view_id } => (*view_id, depth),
            Self::Split { first, second, .. } => {
                let first_leaf = first.shallowest_leaf_at_depth(depth + 1);
                let second_leaf = second.shallowest_leaf_at_depth(depth + 1);
                if first_leaf.1 <= second_leaf.1 {
                    first_leaf
                } else {
                    second_leaf
                }
            }
        }
    }
}
