use super::{PaneBranch, PaneNode, SplitAxis, TileDirection};
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

    pub fn resize_view_in_direction(
        &mut self,
        view_id: ViewId,
        direction: TileDirection,
        delta: f32,
    ) -> bool {
        let Some(leaf_path) = self.path_to_view(view_id) else {
            return false;
        };
        let Some((split_path, _)) = self.nearest_split_for_direction(&leaf_path, direction, false)
        else {
            return false;
        };
        self.adjust_split_ratio_for_direction(&split_path, direction, delta.abs())
    }

    pub fn move_view_in_direction(&mut self, view_id: ViewId, direction: TileDirection) -> bool {
        let Some(active_path) = self.path_to_view(view_id) else {
            return false;
        };
        let Some((split_path, active_branch)) =
            self.nearest_split_for_direction(&active_path, direction, true)
        else {
            return false;
        };

        let sibling_path = branched_path(&split_path, opposite_branch(active_branch));
        let Some(target_path) = self.neighbor_leaf_path(&sibling_path, direction) else {
            return false;
        };
        let Some(target_view_id) = self.leaf_view_id_at_path(&target_path) else {
            return false;
        };

        self.set_leaf_view_id_at_path(&active_path, target_view_id)
            && self.set_leaf_view_id_at_path(&target_path, view_id)
    }

    #[must_use]
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

fn opposite_branch(branch: PaneBranch) -> PaneBranch {
    match branch {
        PaneBranch::First => PaneBranch::Second,
        PaneBranch::Second => PaneBranch::First,
    }
}

fn branched_path(path: &[PaneBranch], branch: PaneBranch) -> Vec<PaneBranch> {
    let mut next = Vec::with_capacity(path.len() + 1);
    next.extend_from_slice(path);
    next.push(branch);
    next
}

impl TileDirection {
    fn axis(self) -> SplitAxis {
        match self {
            Self::Left | Self::Right => SplitAxis::Vertical,
            Self::Up | Self::Down => SplitAxis::Horizontal,
        }
    }

    fn branch_that_can_grow_toward(self) -> PaneBranch {
        match self {
            Self::Left | Self::Up => PaneBranch::Second,
            Self::Right | Self::Down => PaneBranch::First,
        }
    }

    fn uses_previous_sibling_edge(self) -> bool {
        matches!(self, Self::Left | Self::Up)
    }
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

    fn child(&self, branch: PaneBranch) -> Option<&PaneNode> {
        match (self, branch) {
            (Self::Split { first, .. }, PaneBranch::First) => Some(first.as_ref()),
            (Self::Split { second, .. }, PaneBranch::Second) => Some(second.as_ref()),
            (Self::Leaf { .. }, _) => None,
        }
    }

    fn path_to_view(&self, target: ViewId) -> Option<Vec<PaneBranch>> {
        let mut path = Vec::new();
        self.collect_path_to_view(target, &mut path).then_some(path)
    }

    fn collect_path_to_view(&self, target: ViewId, path: &mut Vec<PaneBranch>) -> bool {
        match self {
            Self::Leaf { view_id } => *view_id == target,
            Self::Split { first, second, .. } => {
                path.push(PaneBranch::First);
                if first.collect_path_to_view(target, path) {
                    return true;
                }
                path.pop();

                path.push(PaneBranch::Second);
                if second.collect_path_to_view(target, path) {
                    return true;
                }
                path.pop();
                false
            }
        }
    }

    fn nearest_split_for_direction(
        &self,
        leaf_path: &[PaneBranch],
        direction: TileDirection,
        require_growable_branch: bool,
    ) -> Option<(Vec<PaneBranch>, PaneBranch)> {
        for split_depth in (0..leaf_path.len()).rev() {
            let split_path = &leaf_path[..split_depth];
            let active_branch = leaf_path[split_depth];
            if require_growable_branch && active_branch != direction.branch_that_can_grow_toward() {
                continue;
            }
            if self
                .node_at_path(split_path)
                .is_some_and(|node| node.split_axis() == Some(direction.axis()))
            {
                return Some((split_path.to_vec(), active_branch));
            }
        }
        None
    }

    fn node_at_path(&self, path: &[PaneBranch]) -> Option<&PaneNode> {
        let mut node = self;
        for branch in path {
            node = node.child(*branch)?;
        }
        Some(node)
    }

    fn node_mut_at_path(&mut self, path: &[PaneBranch]) -> Option<&mut PaneNode> {
        let mut node = self;
        for branch in path {
            node = node.child_mut(*branch)?;
        }
        Some(node)
    }

    fn split_axis(&self) -> Option<SplitAxis> {
        match self {
            Self::Split { axis, .. } => Some(*axis),
            Self::Leaf { .. } => None,
        }
    }

    fn adjust_split_ratio_for_direction(
        &mut self,
        split_path: &[PaneBranch],
        direction: TileDirection,
        delta: f32,
    ) -> bool {
        let Some(Self::Split { ratio, .. }) = self.node_mut_at_path(split_path) else {
            return false;
        };
        let before = *ratio;
        let signed_delta = match direction {
            TileDirection::Left | TileDirection::Up => -delta,
            TileDirection::Right | TileDirection::Down => delta,
        };
        *ratio = clamp_split_ratio(*ratio + signed_delta);
        (*ratio - before).abs() > f32::EPSILON
    }

    fn neighbor_leaf_path(
        &self,
        sibling_path: &[PaneBranch],
        direction: TileDirection,
    ) -> Option<Vec<PaneBranch>> {
        if direction.uses_previous_sibling_edge() {
            self.last_leaf_path_from(sibling_path)
        } else {
            self.first_leaf_path_from(sibling_path)
        }
    }

    fn first_leaf_path_from(&self, path: &[PaneBranch]) -> Option<Vec<PaneBranch>> {
        let mut leaf_path = path.to_vec();
        let mut node = self.node_at_path(path)?;
        while let Self::Split { first, .. } = node {
            leaf_path.push(PaneBranch::First);
            node = first.as_ref();
        }
        Some(leaf_path)
    }

    fn last_leaf_path_from(&self, path: &[PaneBranch]) -> Option<Vec<PaneBranch>> {
        let mut leaf_path = path.to_vec();
        let mut node = self.node_at_path(path)?;
        while let Self::Split { second, .. } = node {
            leaf_path.push(PaneBranch::Second);
            node = second.as_ref();
        }
        Some(leaf_path)
    }

    fn leaf_view_id_at_path(&self, path: &[PaneBranch]) -> Option<ViewId> {
        match self.node_at_path(path)? {
            Self::Leaf { view_id } => Some(*view_id),
            Self::Split { .. } => None,
        }
    }

    fn set_leaf_view_id_at_path(&mut self, path: &[PaneBranch], replacement: ViewId) -> bool {
        match self.node_mut_at_path(path) {
            Some(Self::Leaf { view_id }) => {
                *view_id = replacement;
                true
            }
            _ => false,
        }
    }
}
