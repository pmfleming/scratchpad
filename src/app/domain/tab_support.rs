use crate::app::domain::{BufferId, BufferState, EditorViewState, PaneNode, ViewId};
use std::collections::{HashMap, HashSet};

pub(crate) struct ViewPromotionPlan {
    pub(crate) promoted_buffer_id: BufferId,
    pub(crate) promoted_view_ids: HashSet<ViewId>,
    pub(crate) promoted_root: PaneNode,
    pub(crate) promoted_active_view_id: ViewId,
    pub(crate) remaining_active_view_id: ViewId,
    pub(crate) replacement_buffer_id: BufferId,
}

pub(crate) struct FileTabParts {
    pub(crate) buffer: BufferState,
    pub(crate) views: Vec<EditorViewState>,
    pub(crate) root_pane: PaneNode,
    pub(crate) active_view_id: ViewId,
}

pub(crate) fn ordered_buffer_ids(
    views: &[EditorViewState],
    ordered_view_ids: &[ViewId],
) -> Vec<BufferId> {
    let buffer_by_view_id = views
        .iter()
        .map(|view| (view.id, view.buffer_id))
        .collect::<HashMap<_, _>>();
    let mut ordered_buffer_ids = Vec::new();
    let mut seen_buffer_ids = HashSet::new();

    for view_id in ordered_view_ids {
        if let Some(buffer_id) = buffer_by_view_id.get(view_id)
            && seen_buffer_ids.insert(*buffer_id)
        {
            ordered_buffer_ids.push(*buffer_id);
        }
    }

    ordered_buffer_ids
}

pub(crate) fn append_missing_buffer_ids(
    ordered_buffer_ids: &mut Vec<BufferId>,
    views: &[EditorViewState],
) {
    let mut seen_buffer_ids = ordered_buffer_ids.iter().copied().collect::<HashSet<_>>();
    for buffer_id in views.iter().map(|view| view.buffer_id) {
        if seen_buffer_ids.insert(buffer_id) {
            ordered_buffer_ids.push(buffer_id);
        }
    }
}

pub(crate) fn group_views_by_buffer(
    views: Vec<EditorViewState>,
) -> HashMap<BufferId, Vec<EditorViewState>> {
    views.into_iter().fold(HashMap::new(), |mut groups, view| {
        groups.entry(view.buffer_id).or_default().push(view);
        groups
    })
}

pub(crate) fn view_order_lookup(ordered_view_ids: &[ViewId]) -> HashMap<ViewId, usize> {
    ordered_view_ids
        .iter()
        .enumerate()
        .map(|(index, view_id)| (*view_id, index))
        .collect()
}

pub(crate) fn take_file_tab_parts(
    buffer_id: BufferId,
    root_pane: &PaneNode,
    active_view_id: ViewId,
    active_buffer_id: Option<BufferId>,
    buffers: &mut HashMap<BufferId, BufferState>,
    views_by_buffer: &mut HashMap<BufferId, Vec<EditorViewState>>,
    view_order: &HashMap<ViewId, usize>,
) -> Option<FileTabParts> {
    let buffer = buffers.remove(&buffer_id)?;
    let mut views = views_by_buffer.remove(&buffer_id)?;
    sort_views_by_layout_order(&mut views, view_order);
    let root_pane = file_root_pane(root_pane, &views);
    let active_view_id = file_active_view_id(
        buffer_id,
        active_buffer_id,
        active_view_id,
        &views,
        &root_pane,
    );

    Some(FileTabParts {
        buffer,
        views,
        root_pane,
        active_view_id,
    })
}

fn sort_views_by_layout_order(views: &mut [EditorViewState], view_order: &HashMap<ViewId, usize>) {
    views.sort_by_key(|view| view_order.get(&view.id).copied().unwrap_or(usize::MAX));
}

fn file_root_pane(root_pane: &PaneNode, views: &[EditorViewState]) -> PaneNode {
    let file_view_ids = views.iter().map(|view| view.id).collect::<HashSet<_>>();
    let mut file_root = root_pane.clone();
    if !file_root.retain_views(&file_view_ids) {
        PaneNode::leaf(views[0].id)
    } else {
        file_root
    }
}

fn file_active_view_id(
    buffer_id: BufferId,
    active_buffer_id: Option<BufferId>,
    active_view_id: ViewId,
    views: &[EditorViewState],
    root_pane: &PaneNode,
) -> ViewId {
    let file_view_ids = views.iter().map(|view| view.id).collect::<HashSet<_>>();
    if active_buffer_id == Some(buffer_id) && file_view_ids.contains(&active_view_id) {
        active_view_id
    } else {
        root_pane.first_view_id()
    }
}
