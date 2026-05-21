use super::{PROFILE_QUERY, SEARCH_VIEW_DUPLICATES_PER_TAB};
use crate::ScratchpadApp;
use crate::app::commands::{AppCommand, WorkspaceCommand};
use crate::app::domain::{
    BufferState, PaneBranch, PaneNode, SplitAxis, SplitPath, ViewId, WorkspaceTab,
};
use crate::app::services::search::{SearchOptions, find_matches};
use crate::app::services::session_store::SessionStore;
use std::hint::black_box;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) fn sum_profile_iterations(
    mut iterations: usize,
    mut run_iteration: impl FnMut() -> usize,
) -> usize {
    let mut total = 0;
    while iterations > 0 {
        total += black_box(run_iteration());
        iterations -= 1;
    }
    total
}

pub(super) fn with_isolated_app<T>(label: &str, run: impl FnOnce(&mut ScratchpadApp) -> T) -> T {
    with_profile_app(label, true, run)
}

pub(super) fn with_steady_state_app<T>(
    label: &str,
    run: impl FnOnce(&mut ScratchpadApp) -> T,
) -> T {
    with_profile_app(label, false, run)
}

pub(super) fn wait_for_app_state_search_matches(app: &mut ScratchpadApp, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        app.poll_search();
        if app.state.search_state.match_count() == expected {
            return;
        }
        thread::yield_now();
    }

    panic!(
        "timed out waiting for {expected} search matches; got {}",
        app.state.search_state.match_count()
    );
}

pub(super) fn plain_text_of_size(target_bytes: usize) -> String {
    repeat_line_to_target_size(
        "The quick brown fox jumps over the lazy dog 0123456789.\n",
        target_bytes,
    )
}

pub(super) fn install_navigation_workspace(
    app: &mut ScratchpadApp,
    tab_count: usize,
    views_per_tab: usize,
    bytes_per_buffer: usize,
) {
    let total_tabs = tab_count.max(1);
    app.tab_manager.tabs.as_mut_slice()[0] =
        build_view_dense_tab(0, views_per_tab, bytes_per_buffer);
    for tab_index in 1..total_tabs {
        crate::app::app_state::workspace_controller::append_tab(
            app,
            build_view_dense_tab(tab_index, views_per_tab, bytes_per_buffer),
        );
    }
    crate::app::commands::handle_command(
        app,
        AppCommand::Workspace(WorkspaceCommand::ActivateTab { index: 0 }),
    );
}

pub(super) fn install_profile_tab<T>(
    app: &mut ScratchpadApp,
    tab: WorkspaceTab,
    inspect: impl FnOnce(&WorkspaceTab) -> T,
) -> T {
    let result = inspect(&tab);
    app.tab_manager.tabs.as_mut_slice()[0] = tab;
    result
}

pub(super) fn install_search_all_tabs(
    app: &mut ScratchpadApp,
    tab_count: usize,
    bytes_per_tab: usize,
) -> usize {
    let total_tabs = tab_count.max(1);
    let mut target_match_count = install_profile_tab(
        app,
        build_search_all_tab(0, bytes_per_tab),
        target_match_count_for_tab,
    );

    for tab_index in 1..total_tabs {
        let tab = build_search_all_tab(tab_index, bytes_per_tab);
        target_match_count += target_match_count_for_tab(&tab);
        crate::app::app_state::workspace_controller::append_tab(app, tab);
    }

    crate::app::commands::handle_command(
        app,
        AppCommand::Workspace(WorkspaceCommand::ActivateTab { index: 0 }),
    );
    target_match_count
}

pub(super) fn build_search_current_scope_tab(
    file_count: usize,
    bytes_per_file: usize,
) -> WorkspaceTab {
    let mut tab = build_balanced_tile_tab(0, file_count.max(1), bytes_per_file);
    let primary_view_id = tab.layout.root_pane.first_view_id();
    duplicate_primary_view(&mut tab, primary_view_id, 0);
    tab
}

pub(super) fn build_view_dense_tab(
    tab_index: usize,
    view_count: usize,
    bytes_per_buffer: usize,
) -> WorkspaceTab {
    let total_views = view_count.max(1);
    let mut tab = WorkspaceTab::new(corpus_buffer(
        format!("tab_{tab_index}_root.rs"),
        tab_index,
        bytes_per_buffer,
    ));
    let primary_view_id = tab.layout.active_view_id;

    for view_index in 1..total_views {
        let axis = alternating_axis(tab_index + view_index);
        if view_index.is_multiple_of(3) {
            tab.activate_view(primary_view_id);
            let _ = tab.split_active_view(axis);
            continue;
        }

        let _ = tab.open_buffer_with_balanced_layout(BufferState::new(
            format!("tab_{tab_index}_buffer_{view_index}.rs"),
            corpus_text_of_size(tab_index * 1000 + view_index, bytes_per_buffer),
            None,
        ));
    }

    tab.activate_view(primary_view_id);
    tab
}

pub(super) fn build_balanced_tile_tab(
    tab_index: usize,
    tile_count: usize,
    bytes_per_tile: usize,
) -> WorkspaceTab {
    let total_tiles = tile_count.max(1);
    let mut tab = WorkspaceTab::new(plain_text_buffer(
        format!("tab_{tab_index}_tile_0.txt"),
        bytes_per_tile,
    ));

    for tile_index in 1..total_tiles {
        let _ = tab.open_buffer_with_balanced_layout(plain_text_buffer(
            format!("tab_{tab_index}_tile_{tile_index}.txt"),
            bytes_per_tile,
        ));
    }

    tab
}

pub(super) fn target_match_count_for_tab(tab: &WorkspaceTab) -> usize {
    tab.buffers()
        .map(|buffer| find_matches(&buffer.text(), PROFILE_QUERY, SearchOptions::default()).len())
        .sum()
}

pub(super) fn ordered_view_ids(root_pane: &PaneNode) -> Vec<ViewId> {
    let mut ordered = Vec::new();
    root_pane.collect_view_ids_in_order(&mut ordered);
    ordered
}

pub(super) fn collect_split_paths(root_pane: &PaneNode) -> Vec<SplitPath> {
    let mut current = Vec::new();
    let mut paths = Vec::new();
    collect_split_paths_inner(root_pane, &mut current, &mut paths);
    paths
}

pub(super) fn resize_profile_splits(
    app: &mut ScratchpadApp,
    split_paths: &[SplitPath],
    ratio_phase: bool,
) -> usize {
    let phase = usize::from(ratio_phase);
    for (index, path) in split_paths.iter().enumerate() {
        let ratio = if (index + phase).is_multiple_of(2) {
            0.35
        } else {
            0.65
        };
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ResizeSplit {
                path: path.clone(),
                ratio,
            }),
        );
    }
    split_paths.len()
}

pub(super) fn rebalance_profile_tab(app: &mut ScratchpadApp) -> usize {
    app.tab_manager
        .tabs
        .as_mut_slice()
        .first_mut()
        .map(rebalance_profile_tab_views)
        .unwrap_or(0)
}

pub(super) fn rebalance_profile_tab_views(tab: &mut WorkspaceTab) -> usize {
    let _ = tab.rebalance_views_equally();
    let _ = tab.rebalance_views_equally_for_axis(SplitAxis::Horizontal);
    tab.layout.views.len()
}

pub(super) fn cycle_profile_views(app: &mut ScratchpadApp, view_ids: &[ViewId]) -> usize {
    let mut activations = 0;
    for &view_id in view_ids.iter().skip(1) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ActivateView { view_id }),
        );
        activations += 1;
    }
    for &view_id in view_ids.iter().rev().skip(1) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ActivateView { view_id }),
        );
        activations += 1;
    }
    activations
}

pub(super) fn bouncing_indices(count: usize) -> Vec<usize> {
    match count {
        0 => Vec::new(),
        1 => vec![0],
        _ => {
            let mut indices = (1..count).collect::<Vec<_>>();
            indices.extend((0..count - 1).rev());
            indices
        }
    }
}

pub(super) fn alternating_axis(index: usize) -> SplitAxis {
    if index.is_multiple_of(2) {
        SplitAxis::Vertical
    } else {
        SplitAxis::Horizontal
    }
}

fn with_profile_app<T>(
    label: &str,
    cleanup_session_root: bool,
    run: impl FnOnce(&mut ScratchpadApp) -> T,
) -> T {
    let session_root = unique_profile_session_root(label);
    let cleanup_root = cleanup_session_root.then(|| session_root.clone());
    let session_store = SessionStore::new(session_root);
    let mut app = ScratchpadApp::with_session_store(session_store);
    app.set_session_persist_on_drop(false);
    let result = run(&mut app);

    if let Some(root) = cleanup_root {
        drop(app);
        let _ = std::fs::remove_dir_all(root);
    } else {
        std::mem::forget(app);
    }

    result
}

pub(super) fn unique_profile_session_root(label: &str) -> std::path::PathBuf {
    let unique_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "scratchpad-profile-{label}-{}-{unique_suffix}",
        std::process::id()
    ))
}

fn build_search_all_tab(tab_index: usize, bytes_per_tab: usize) -> WorkspaceTab {
    let mut tab = WorkspaceTab::new(corpus_buffer(
        format!("search_tab_{tab_index}.rs"),
        tab_index,
        bytes_per_tab,
    ));
    let primary_view_id = tab.layout.active_view_id;
    duplicate_primary_view(&mut tab, primary_view_id, tab_index);
    tab
}

fn duplicate_primary_view(tab: &mut WorkspaceTab, primary_view_id: ViewId, axis_seed: usize) {
    for offset in 0..SEARCH_VIEW_DUPLICATES_PER_TAB {
        tab.activate_view(primary_view_id);
        let _ = tab.split_active_view(alternating_axis(axis_seed + offset));
    }
    tab.activate_view(primary_view_id);
}

fn corpus_buffer(name: String, item_index: usize, target_bytes: usize) -> BufferState {
    BufferState::new(name, corpus_text_of_size(item_index, target_bytes), None)
}

fn plain_text_buffer(name: String, target_bytes: usize) -> BufferState {
    BufferState::new(name, plain_text_of_size(target_bytes), None)
}

fn corpus_text_of_size(item_index: usize, target_bytes: usize) -> String {
    repeat_line_to_target_size(
        &format!(
            "item {item_index} needle alpha beta gamma {}\n",
            "x".repeat(48)
        ),
        target_bytes,
    )
}

fn repeat_line_to_target_size(line: &str, target_bytes: usize) -> String {
    let repeats = (target_bytes / line.len()).max(1);
    let mut text = String::with_capacity(repeats * line.len());
    for _ in 0..repeats {
        text.push_str(line);
    }
    text
}

fn collect_split_paths_inner(node: &PaneNode, current: &mut SplitPath, paths: &mut Vec<SplitPath>) {
    if let PaneNode::Split { first, second, .. } = node {
        paths.push(current.clone());

        current.push(PaneBranch::First);
        collect_split_paths_inner(first, current, paths);
        current.pop();

        current.push(PaneBranch::Second);
        collect_split_paths_inner(second, current, paths);
        current.pop();
    }
}
