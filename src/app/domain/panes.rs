use crate::app::domain::ViewId;
use std::collections::HashSet;

mod split;

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
