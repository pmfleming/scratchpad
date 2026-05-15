use crate::app::app_state::{
    ScratchpadApp, workspace::accessors as workspace_accessors, workspace_controller,
};
use crate::app::domain::{SplitAxis, ViewId, WorkspaceTab};

struct TabCombineContext {
    adjusted_target_index: usize,
}

pub(super) fn combine_tab_into_tab_command(
    app: &mut ScratchpadApp,
    source_index: usize,
    target_index: usize,
) {
    if !can_combine_tabs(
        app.tab_manager.tabs.as_slice().len(),
        source_index,
        target_index,
    ) {
        return;
    }

    if source_index == app.tab_manager.active_tab_index
        || target_index == app.tab_manager.active_tab_index
    {
        app.reload_settings_before_workspace_change();
    }

    let (context, source_tab) = remove_source_tab_for_combine(app, source_index, target_index);
    let mut source_tab = Some(source_tab);
    if !try_combine_tabs(app, context.adjusted_target_index, &mut source_tab) {
        rollback_combined_tab(
            app,
            source_index,
            source_tab.expect("source tab should remain available on combine failure"),
        );
        return;
    }

    crate::app::app_state::frame::begin_layout_transition(app);
    rebalance_combined_workspace_layout(app, context.adjusted_target_index, target_index);
    finish_combined_tab(app, source_index, target_index, context);
}

pub(super) fn promote_view_to_tab_command(app: &mut ScratchpadApp, view_id: ViewId) {
    app.reload_settings_before_workspace_change();

    let source_index = app.tab_manager.active_tab_index;
    let promoted_tab = app
        .tab_manager
        .tabs
        .as_mut_slice()
        .get_mut(source_index)
        .and_then(|tab| tab.promote_view_to_new_tab(view_id));
    let Some(promoted_tab) = promoted_tab else {
        return;
    };

    crate::app::app_state::frame::begin_layout_transition(app);
    workspace_controller::append_tab(app, promoted_tab);
    let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
}

pub(super) fn promote_tab_files_to_tabs_command(app: &mut ScratchpadApp, index: usize) {
    if index >= app.tab_manager.tabs.as_slice().len() {
        return;
    }

    if index == app.tab_manager.active_tab_index {
        app.reload_settings_before_workspace_change();
    }
    let source_tab = app.tab_manager.tabs.remove(index);
    if !crate::app::domain::tab::summary::can_promote_all_files(&source_tab) {
        app.tab_manager.tabs.insert(index, source_tab);
        app.tab_manager.rebuild_buffer_tab_index();
        return;
    }

    let active_buffer_id = source_tab.active_buffer().id;
    let promoted_tabs = source_tab.into_tabs_per_file();
    if promoted_tabs.len() <= 1 {
        app.tab_manager.tabs.insert(
            index,
            promoted_tabs
                .into_iter()
                .next()
                .unwrap_or_else(WorkspaceTab::untitled),
        );
        app.tab_manager.rebuild_buffer_tab_index();
        return;
    }

    crate::app::app_state::frame::begin_layout_transition(app);
    let active_tab_offset = promoted_tabs
        .iter()
        .position(|tab| tab.active_buffer().id == active_buffer_id)
        .unwrap_or(0);
    for (offset, tab) in promoted_tabs.into_iter().enumerate() {
        app.tab_manager.tabs.insert(index + offset, tab);
    }
    app.tab_manager
        .set_active_tab_index_clamped(index + active_tab_offset);
    app.tab_manager.rebuild_buffer_tab_index();
    crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);
    app.tab_manager.pending_scroll_to_active = true;
    workspace_accessors::request_focus_for_active_view(app);
    app.tab_manager.mark_session_dirty();
    let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
}

fn can_combine_tabs(tab_count: usize, source_index: usize, target_index: usize) -> bool {
    source_index != target_index && source_index < tab_count && target_index < tab_count
}

pub(super) fn combine_tabs_into_tab_command(
    app: &mut ScratchpadApp,
    mut source_indices: Vec<usize>,
    target_index: usize,
) {
    source_indices.sort_unstable();
    source_indices.dedup();
    source_indices.retain(|index| *index != target_index);
    if source_indices.is_empty()
        || source_indices
            .iter()
            .any(|index| *index >= app.tab_manager.tabs.as_slice().len())
        || target_index >= app.tab_manager.tabs.as_slice().len()
    {
        return;
    }

    if source_indices.contains(&app.tab_manager.active_tab_index)
        || target_index == app.tab_manager.active_tab_index
    {
        app.reload_settings_before_workspace_change();
    }

    if app.tab_manager.tabs.get(target_index).is_none() {
        return;
    }

    let mut moved_tabs = Vec::with_capacity(source_indices.len());
    let mut adjusted_target_index = target_index;
    for source_index in source_indices.iter().rev().copied() {
        let removed = app.tab_manager.tabs.remove(source_index);
        if source_index < adjusted_target_index {
            adjusted_target_index = adjusted_target_index.saturating_sub(1);
        }
        moved_tabs.push(removed);
    }
    moved_tabs.reverse();

    {
        let Some(target_tab) = app.tab_manager.tabs.get_mut(adjusted_target_index) else {
            app.tab_manager.rebuild_buffer_tab_index();
            return;
        };

        for source_tab in moved_tabs {
            let _ = target_tab.combine_with_tab(source_tab, SplitAxis::Vertical, false, 0.5);
        }
    }

    crate::app::app_state::frame::begin_layout_transition(app);
    app.tab_manager
        .set_active_tab_index_clamped(adjusted_target_index);
    app.tab_manager.rebuild_buffer_tab_index();
    crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);
    app.tab_manager.pending_scroll_to_active = true;
    workspace_accessors::request_focus_for_active_view(app);
    app.tab_manager.mark_session_dirty();
    rebalance_combined_workspace_layout(app, adjusted_target_index, target_index);
    let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
}

fn remove_source_tab_for_combine(
    app: &mut ScratchpadApp,
    source_index: usize,
    target_index: usize,
) -> (TabCombineContext, WorkspaceTab) {
    let adjusted_target_index = adjusted_target_index(source_index, target_index);
    let source_tab = app.tab_manager.tabs.remove(source_index);
    (
        TabCombineContext {
            adjusted_target_index,
        },
        source_tab,
    )
}

fn adjusted_target_index(source_index: usize, target_index: usize) -> usize {
    if source_index < target_index {
        target_index.saturating_sub(1)
    } else {
        target_index
    }
}

fn try_combine_tabs(
    app: &mut ScratchpadApp,
    adjusted_target_index: usize,
    source_tab: &mut Option<WorkspaceTab>,
) -> bool {
    app.tab_manager
        .tabs
        .get_mut(adjusted_target_index)
        .is_some_and(|target_tab| {
            target_tab
                .combine_with_tab(
                    source_tab
                        .take()
                        .expect("source tab removed before combine"),
                    SplitAxis::Vertical,
                    false,
                    0.5,
                )
                .is_some()
        })
}

fn rollback_combined_tab(app: &mut ScratchpadApp, source_index: usize, source_tab: WorkspaceTab) {
    let reinsertion_index = source_index.min(app.tab_manager.tabs.as_slice().len());
    app.tab_manager.tabs.insert(reinsertion_index, source_tab);
    app.tab_manager.rebuild_buffer_tab_index();
}

fn rebalance_combined_workspace_layout(
    app: &mut ScratchpadApp,
    adjusted_target_index: usize,
    _target_index: usize,
) {
    let reflow_axis = app.state.workspace_reflow_axis;
    let Some(target_tab) = app.tab_manager.tabs.get_mut(adjusted_target_index) else {
        return;
    };
    let _ = target_tab.rebalance_views_equally_for_axis(reflow_axis);
}

fn finish_combined_tab(
    app: &mut ScratchpadApp,
    _source_index: usize,
    _target_index: usize,
    context: TabCombineContext,
) {
    app.tab_manager
        .set_active_tab_index_clamped(context.adjusted_target_index);
    app.tab_manager.rebuild_buffer_tab_index();
    crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);
    app.tab_manager.pending_scroll_to_active = true;
    workspace_accessors::request_focus_for_active_view(app);
    app.tab_manager.mark_session_dirty();
}
