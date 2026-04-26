use super::{PaneBranch, PaneNode, SplitAxis};
use crate::app::domain::ViewId;

const MIN_SPLIT_RATIO: f32 = 0.2;
const MAX_SPLIT_RATIO: f32 = 0.8;

impl PaneNode {
    pub fn split_view(
        &mut self,
        target: ViewId,
        axis: SplitAxis,
        new_view_id: ViewId,
        new_view_first: bool,
        ratio: f32,
    ) -> bool {
        self.split_view_with_node(
            target,
            axis,
            PaneNode::leaf(new_view_id),
            new_view_first,
            ratio,
        )
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
                *self = split_leaf_node(axis, ratio, *view_id, new_node, new_view_first);
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
        let clamped_ratio = clamp_split_ratio(ratio);
        match path.split_first() {
            None => self.set_split_ratio(clamped_ratio),
            Some((branch, remainder)) => self
                .child_mut(*branch)
                .is_some_and(|child| child.resize_split(remainder, clamped_ratio)),
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

fn split_leaf_node(
    axis: SplitAxis,
    ratio: f32,
    existing_view_id: ViewId,
    new_node: PaneNode,
    new_view_first: bool,
) -> PaneNode {
    let existing_leaf = Box::new(PaneNode::leaf(existing_view_id));
    let new_node = Box::new(new_node);
    let (first, second) = if new_view_first {
        (new_node, existing_leaf)
    } else {
        (existing_leaf, new_node)
    };

    PaneNode::Split {
        axis,
        ratio: clamp_split_ratio(ratio),
        first,
        second,
    }
}

fn clamp_split_ratio(ratio: f32) -> f32 {
    ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO)
}

impl PaneNode {
    fn set_split_ratio(&mut self, ratio: f32) -> bool {
        match self {
            Self::Split {
                ratio: split_ratio, ..
            } => {
                *split_ratio = ratio;
                true
            }
            Self::Leaf { .. } => false,
        }
    }

    fn child_mut(&mut self, branch: PaneBranch) -> Option<&mut PaneNode> {
        match (self, branch) {
            (Self::Split { first, .. }, PaneBranch::First) => Some(first.as_mut()),
            (Self::Split { second, .. }, PaneBranch::Second) => Some(second.as_mut()),
            (Self::Leaf { .. }, _) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MIN_SPLIT_RATIO, PaneBranch, PaneNode, SplitAxis};

    #[test]
    fn pane_node_split_view_clamps_ratio_and_inserts_leaf() {
        let mut pane = PaneNode::leaf(1);

        assert!(pane.split_view(1, SplitAxis::Horizontal, 2, false, 0.05));

        match pane {
            PaneNode::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                assert_eq!(axis, SplitAxis::Horizontal);
                assert_eq!(ratio, MIN_SPLIT_RATIO);
                assert!(matches!(first.as_ref(), PaneNode::Leaf { view_id: 1 }));
                assert!(matches!(second.as_ref(), PaneNode::Leaf { view_id: 2 }));
            }
            PaneNode::Leaf { .. } => panic!("expected split pane"),
        }
    }

    #[test]
    fn pane_node_resize_split_follows_branch_path() {
        let mut pane = PaneNode::Split {
            axis: SplitAxis::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio: 0.5,
                first: Box::new(PaneNode::leaf(1)),
                second: Box::new(PaneNode::leaf(2)),
            }),
            second: Box::new(PaneNode::leaf(3)),
        };

        assert!(pane.resize_split(&[PaneBranch::First], 0.7));

        match pane {
            PaneNode::Split { first, .. } => match first.as_ref() {
                PaneNode::Split { ratio, .. } => assert_eq!(*ratio, 0.7),
                PaneNode::Leaf { .. } => panic!("expected nested split"),
            },
            PaneNode::Leaf { .. } => panic!("expected root split"),
        }
    }
}
