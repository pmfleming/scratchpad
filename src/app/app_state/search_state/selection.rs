use super::{ScratchpadApp, SearchScope};
use crate::app::domain::buffer::{AnchorBias, AnchorId};
use std::ops::Range;
#[derive(Clone, Debug)]
pub(crate) struct LockedSelection {
    pub buffer: u64,
    pub view: u64,
    pub start: AnchorId,
    pub end: AnchorId,
}

pub(crate) fn release(app: &mut ScratchpadApp) {
    if let Some(locked) = app.state.search_state.query.selection.take()
        && let Some(buffer) = app
            .tab_manager
            .tabs
            .as_mut_slice()
            .iter_mut()
            .find_map(|t| t.buffer_by_id_mut(locked.buffer))
    {
        let tree = buffer.document_mut().piece_tree_mut();
        tree.release_anchor(locked.start);
        tree.release_anchor(locked.end);
    }
}
pub(crate) fn capture(app: &mut ScratchpadApp) {
    release(app);
    let Some(range) = app.active_search_selection_range() else {
        return;
    };
    let Some(tab) = app.tab_manager.active_tab_mut() else {
        return;
    };
    let view = tab.layout.active_view_id();
    let Some(buffer) = tab.buffer_and_view_mut(view).map(|(b, _)| b) else {
        return;
    };
    let id = buffer.id;
    let tree = buffer.document_mut().piece_tree_mut();
    let start = tree.create_anchor(range.start, AnchorBias::Left);
    let end = tree.create_anchor(range.end, AnchorBias::Right);
    app.state.search_state.query.selection = Some(LockedSelection {
        buffer: id,
        view,
        start,
        end,
    });
}
pub(crate) fn resolve(app: &ScratchpadApp) -> Option<(usize, u64, u64, Range<usize>)> {
    if app.state.search_state.query.scope != SearchScope::SelectionOnly {
        return None;
    }
    let locked = app.state.search_state.query.selection.as_ref()?;
    let (index, buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .enumerate()
        .find_map(|(i, t)| t.buffer_by_id(locked.buffer).map(|b| (i, b)))?;
    let tree = buffer.document().piece_tree();
    let start = tree.anchor_position(locked.start)?;
    let end = tree.anchor_position(locked.end)?;
    (start < end).then_some((index, locked.buffer, locked.view, start..end))
}
