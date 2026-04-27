use crate::ScratchpadApp;
use crate::app::app_state::SearchScope;
use crate::app::commands::AppCommand;
use crate::app::domain::{
    BufferState, PaneBranch, PaneNode, RenderedLayout, SearchHighlightState, SplitAxis, SplitPath,
    ViewId, WorkspaceTab,
};
use crate::app::services::search::{SearchOptions, find_matches};
use crate::app::services::session_store::SessionStore;
use crate::app::ui::editor_content::{EditorHighlightStyle, build_layouter};
use eframe::egui;
use std::hint::black_box;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const KB: usize = 1024;
pub const MB: usize = 1024 * KB;
pub const RECOMMENDED_TAB_OPERATION_TABS: usize = 64;
pub const RECOMMENDED_TAB_OPERATION_VIEWS_PER_TAB: usize = 10;
pub const RECOMMENDED_TAB_OPERATION_BYTES_PER_BUFFER: usize = 48 * KB;
pub const RECOMMENDED_TAB_OPERATION_ITERATIONS: usize = 64;
pub const RECOMMENDED_TAB_TILE_COUNT: usize = 16;
pub const RECOMMENDED_TAB_TILE_BYTES: usize = 64 * KB;
pub const RECOMMENDED_TAB_TILE_ITERATIONS: usize = 48;
pub const RECOMMENDED_VIEW_NAVIGATION_VIEWS: usize = 24;
pub const RECOMMENDED_VIEW_NAVIGATION_BYTES_PER_BUFFER: usize = 48 * KB;
pub const RECOMMENDED_VIEW_NAVIGATION_ITERATIONS: usize = 120;
pub const RECOMMENDED_SEARCH_CURRENT_FILES: usize = 16;
pub const RECOMMENDED_SEARCH_CURRENT_BYTES_PER_FILE: usize = 24 * KB;
pub const RECOMMENDED_SEARCH_CURRENT_ITERATIONS: usize = 10;
pub const RECOMMENDED_SEARCH_ALL_TABS: usize = 16;
pub const RECOMMENDED_SEARCH_ALL_BYTES_PER_TAB: usize = 16 * KB;
pub const RECOMMENDED_SEARCH_ALL_ITERATIONS: usize = 10;
pub const RECOMMENDED_SEARCH_DISPATCH_CURRENT_FILES: usize = 64;
pub const RECOMMENDED_SEARCH_DISPATCH_ALL_TABS: usize = 64;
pub const RECOMMENDED_SEARCH_DISPATCH_BYTES_PER_ITEM: usize = 24 * KB;
pub const RECOMMENDED_SEARCH_DISPATCH_ITERATIONS: usize = 32;
pub const RECOMMENDED_DOCUMENT_SNAPSHOT_BYTES: usize = 4 * MB;
pub const RECOMMENDED_DOCUMENT_SNAPSHOT_ITERATIONS: usize = 128;
pub const RECOMMENDED_VIEWPORT_EXTRACTION_BYTES: usize = 4 * MB;
pub const RECOMMENDED_VIEWPORT_EXTRACTION_ITERATIONS: usize = 96;
pub const RECOMMENDED_SCROLL_STRESS_BYTES: usize = MB;
pub const RECOMMENDED_SCROLL_STRESS_ITERATIONS: usize = 28;
pub const RECOMMENDED_PASTE_STRESS_BASE_BYTES: usize = MB;
pub const RECOMMENDED_PASTE_STRESS_INSERT_BYTES: usize = 128 * KB;
pub const RECOMMENDED_PASTE_STRESS_ITERATIONS: usize = 20;
pub const RECOMMENDED_SPLIT_STRESS_TILES: usize = 12;
pub const RECOMMENDED_SPLIT_STRESS_BYTES_PER_TILE: usize = 256 * KB;
pub const RECOMMENDED_SPLIT_STRESS_ITERATIONS: usize = 24;

const PROFILE_QUERY: &str = "needle";
const PROFILE_RESET_QUERY: &str = "zzzz-no-match";
const SEARCH_VIEW_DUPLICATES_PER_TAB: usize = 4;

pub fn run_tab_operations_profile(tab_count: usize, iterations: usize) -> usize {
    with_steady_state_app("tab-operations", |app| {
        install_navigation_workspace(
            app,
            tab_count,
            RECOMMENDED_TAB_OPERATION_VIEWS_PER_TAB,
            RECOMMENDED_TAB_OPERATION_BYTES_PER_BUFFER,
        );
        let tab_order = bouncing_indices(app.tabs().len());

        sum_profile_iterations(iterations, || {
            let mut operations = 0;
            for &index in &tab_order {
                app.handle_command(AppCommand::ActivateTab { index });
                operations += 1;
            }

            if app.tabs().len() > 2 {
                let last_index = app.tabs().len() - 1;
                app.reorder_tab(1, last_index);
                app.reorder_tab(last_index, 1);
                operations += 2;
            }

            operations
        })
    })
}

pub fn run_tab_tile_layout_profile(
    tile_count: usize,
    bytes_per_tile: usize,
    iterations: usize,
) -> usize {
    with_steady_state_app("tab-tile-layout", |app| {
        let tab = build_balanced_tile_tab(0, tile_count, bytes_per_tile);
        let split_paths = collect_split_paths(&tab.root_pane);
        app.tabs_mut()[0] = tab;
        let mut ratio_phase = false;

        sum_profile_iterations(iterations, || {
            ratio_phase = !ratio_phase;
            resize_profile_splits(app, &split_paths, ratio_phase) + rebalance_profile_tab(app)
        })
    })
}

pub fn run_view_navigation_profile(
    view_count: usize,
    bytes_per_buffer: usize,
    iterations: usize,
) -> usize {
    with_steady_state_app("view-navigation", |app| {
        let tab = build_view_dense_tab(0, view_count, bytes_per_buffer);
        let view_ids = ordered_view_ids(&tab.root_pane);
        app.tabs_mut()[0] = tab;

        sum_profile_iterations(iterations, || cycle_profile_views(app, &view_ids))
    })
}

pub fn run_search_current_app_state_profile(
    file_count: usize,
    bytes_per_file: usize,
    iterations: usize,
) -> usize {
    with_isolated_app("search-current-app-state", |app| {
        let tab = build_search_current_scope_tab(file_count, bytes_per_file);
        let expected_matches = expected_matches_for_tab(&tab);
        app.tabs_mut()[0] = tab;

        run_search_profile_iterations(
            app,
            SearchScope::ActiveWorkspaceTab,
            expected_matches,
            iterations,
        )
    })
}

pub fn run_search_all_tabs_profile(
    tab_count: usize,
    bytes_per_tab: usize,
    iterations: usize,
) -> usize {
    with_isolated_app("search-all-tabs", |app| {
        let total_tabs = tab_count.max(1);
        let first_tab = build_search_all_tab(0, bytes_per_tab);
        let mut expected_matches = expected_matches_for_tab(&first_tab);
        app.tabs_mut()[0] = first_tab;

        for tab_index in 1..total_tabs {
            let tab = build_search_all_tab(tab_index, bytes_per_tab);
            expected_matches += expected_matches_for_tab(&tab);
            app.append_tab(tab);
        }

        app.handle_command(AppCommand::ActivateTab { index: 0 });

        run_search_profile_iterations(app, SearchScope::AllOpenTabs, expected_matches, iterations)
    })
}

pub fn run_search_dispatch_current_profile(
    file_count: usize,
    bytes_per_file: usize,
    iterations: usize,
) -> usize {
    with_isolated_app("search-dispatch-current", |app| {
        app.tabs_mut()[0] = build_search_current_scope_tab(file_count, bytes_per_file);
        sum_profile_iterations(iterations, || {
            black_box(
                app.profile_build_search_request(SearchScope::ActiveWorkspaceTab, PROFILE_QUERY),
            )
        })
    })
}

pub fn run_search_dispatch_all_tabs_profile(
    tab_count: usize,
    bytes_per_tab: usize,
    iterations: usize,
) -> usize {
    with_isolated_app("search-dispatch-all", |app| {
        let total_tabs = tab_count.max(1);
        app.tabs_mut()[0] = build_search_all_tab(0, bytes_per_tab);
        for tab_index in 1..total_tabs {
            app.append_tab(build_search_all_tab(tab_index, bytes_per_tab));
        }

        sum_profile_iterations(iterations, || {
            black_box(app.profile_build_search_request(SearchScope::AllOpenTabs, PROFILE_QUERY))
        })
    })
}

pub fn run_document_snapshot_profile(bytes: usize, iterations: usize) -> usize {
    let buffer = BufferState::new(
        "document_snapshot_profile.txt".to_owned(),
        plain_text_of_size(bytes),
        None,
    );

    sum_profile_iterations(iterations, || {
        let snapshot = buffer.document_snapshot();
        black_box(snapshot.len_chars() + snapshot.revision() as usize)
    })
}

pub fn run_viewport_extraction_profile(bytes: usize, iterations: usize) -> usize {
    let buffer = BufferState::new(
        "viewport_extraction_profile.txt".to_owned(),
        plain_text_of_size(bytes),
        None,
    );
    let viewport_lines = 48usize;
    let overscan_lines = 12usize;
    let line_step = 17usize;
    let line_count = buffer.line_count.max(1);
    let mut line_start = 0usize;

    sum_profile_iterations(iterations, || {
        let end = (line_start + viewport_lines + overscan_lines).min(line_count);
        let line_window = buffer.visible_line_window(line_start..end);
        let row_window = (line_window.layout_row_offset)
            ..(line_window.layout_row_offset + line_window.line_range.len());
        let visible_window =
            buffer.visible_text_window(row_window, line_window.char_range.clone(), line_count);

        line_start = if end >= line_count {
            0
        } else {
            (line_start + line_step).min(line_count.saturating_sub(1))
        };

        black_box(
            line_window.text.len()
                + visible_window.text.len()
                + visible_window
                    .char_range
                    .end
                    .saturating_sub(visible_window.char_range.start),
        )
    })
}

pub fn run_scroll_stress_profile(bytes: usize, iterations: usize) -> usize {
    let text = plain_text_of_size(bytes);
    let ctx = egui::Context::default();
    let font_id = egui::FontId::monospace(15.0);
    let highlight_style =
        EditorHighlightStyle::new(egui::Color32::from_rgb(90, 146, 214), egui::Color32::WHITE);
    let text_char_len = text.chars().count();
    let highlight_start = (text_char_len / 7).max(1);
    let highlight_end = (highlight_start + 48).min(text_char_len);
    let selection_start = (text_char_len / 3).max(1);
    let selection_end = (selection_start + 96).min(text_char_len);
    let mut search_highlights = SearchHighlightState::default();
    search_highlights
        .ranges
        .push(highlight_start..highlight_end);
    search_highlights.active_range_index = Some(0);

    let selection = selection_start..selection_end;

    sum_profile_iterations(iterations, || {
        let mut total_rows = 0usize;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                let mut layouter = build_layouter(
                    font_id.clone(),
                    false,
                    egui::Color32::WHITE,
                    highlight_style,
                    search_highlights.clone(),
                    Some(selection.clone()),
                );

                for wrap_width in [980.0, 720.0, 520.0, 980.0] {
                    let galley = layouter(ui, &text, wrap_width);
                    total_rows += RenderedLayout::from_galley(galley).visual_row_count();
                }
            });
        });
        total_rows
    })
}

pub fn run_paste_stress_profile(
    base_bytes: usize,
    insert_bytes: usize,
    iterations: usize,
) -> usize {
    let base_text = plain_text_of_size(base_bytes);
    let insert_text = plain_text_of_size(insert_bytes);
    let insert_char_count = insert_text.chars().count();

    let mut buffer = BufferState::new("paste_stress_profile.txt".to_owned(), base_text, None);

    sum_profile_iterations(iterations, || {
        let midpoint = buffer.document().piece_tree().len_chars() / 2;
        let _ = insert_char_count;
        buffer.document_mut().insert_direct(midpoint, &insert_text);
        buffer.refresh_text_metadata();
        black_box(buffer.line_count + buffer.document().piece_tree().len_bytes())
    })
}

pub fn run_split_stress_profile(
    tile_count: usize,
    bytes_per_tile: usize,
    iterations: usize,
) -> usize {
    with_steady_state_app("split-stress", |app| {
        app.tabs_mut()[0] = build_balanced_tile_tab(0, tile_count, bytes_per_tile);
        let mut axis_seed = 0usize;

        sum_profile_iterations(iterations, || {
            let mut operations = 0usize;
            if let Some(tab) = app.tabs_mut().first_mut() {
                let _ = tab.split_active_view(alternating_axis(axis_seed));
                axis_seed = axis_seed.saturating_add(1);
                operations += 1;

                if tab.views.len() > tile_count
                    && let Some(view_id) = tab.views.last().map(|view| view.id)
                {
                    let _ = tab.close_view(view_id);
                    operations += 1;
                }

                operations += rebalance_profile_tab_views(tab);
            }
            operations
        })
    })
}

fn run_search_profile_iterations(
    app: &mut ScratchpadApp,
    scope: SearchScope,
    expected_matches: usize,
    iterations: usize,
) -> usize {
    app.open_search();
    app.set_search_scope(scope);
    app.set_search_query(PROFILE_RESET_QUERY);
    wait_for_app_state_search_matches(app, 0);

    sum_profile_iterations(iterations, || {
        app.set_search_query(PROFILE_QUERY);
        wait_for_app_state_search_matches(app, expected_matches);
        let match_count = app.search_match_count();

        app.set_search_query(PROFILE_RESET_QUERY);
        wait_for_app_state_search_matches(app, 0);

        match_count
    })
}

fn sum_profile_iterations(
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

fn with_isolated_app<T>(label: &str, run: impl FnOnce(&mut ScratchpadApp) -> T) -> T {
    with_profile_app(label, true, run)
}

fn with_steady_state_app<T>(label: &str, run: impl FnOnce(&mut ScratchpadApp) -> T) -> T {
    with_profile_app(label, false, run)
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

fn unique_profile_session_root(label: &str) -> std::path::PathBuf {
    let unique_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "scratchpad-profile-{label}-{}-{unique_suffix}",
        std::process::id()
    ))
}

fn wait_for_app_state_search_matches(app: &mut ScratchpadApp, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        app.poll_search();
        if app.search_match_count() == expected {
            return;
        }
        thread::yield_now();
    }

    panic!(
        "timed out waiting for {expected} search matches; got {}",
        app.search_match_count()
    );
}

fn plain_text_of_size(target_bytes: usize) -> String {
    repeat_line_to_target_size(
        "The quick brown fox jumps over the lazy dog 0123456789.\n",
        target_bytes,
    )
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

fn install_navigation_workspace(
    app: &mut ScratchpadApp,
    tab_count: usize,
    views_per_tab: usize,
    bytes_per_buffer: usize,
) {
    let total_tabs = tab_count.max(1);
    app.tabs_mut()[0] = build_view_dense_tab(0, views_per_tab, bytes_per_buffer);
    for tab_index in 1..total_tabs {
        app.append_tab(build_view_dense_tab(
            tab_index,
            views_per_tab,
            bytes_per_buffer,
        ));
    }
    app.handle_command(AppCommand::ActivateTab { index: 0 });
}

fn build_search_current_scope_tab(file_count: usize, bytes_per_file: usize) -> WorkspaceTab {
    let mut tab = build_balanced_tile_tab(0, file_count.max(1), bytes_per_file);
    let primary_view_id = tab.root_pane.first_view_id();
    duplicate_primary_view(&mut tab, primary_view_id, 0);
    tab
}

fn build_search_all_tab(tab_index: usize, bytes_per_tab: usize) -> WorkspaceTab {
    let mut tab = WorkspaceTab::new(corpus_buffer(
        format!("search_tab_{tab_index}.rs"),
        tab_index,
        bytes_per_tab,
    ));
    let primary_view_id = tab.active_view_id;
    duplicate_primary_view(&mut tab, primary_view_id, tab_index);
    tab
}

fn build_view_dense_tab(
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
    let primary_view_id = tab.active_view_id;

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

fn build_balanced_tile_tab(
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

fn expected_matches_for_tab(tab: &WorkspaceTab) -> usize {
    tab.buffers()
        .map(|buffer| find_matches(&buffer.text(), PROFILE_QUERY, SearchOptions::default()).len())
        .sum()
}

fn ordered_view_ids(root_pane: &PaneNode) -> Vec<ViewId> {
    let mut ordered = Vec::new();
    root_pane.collect_view_ids_in_order(&mut ordered);
    ordered
}

fn collect_split_paths(root_pane: &PaneNode) -> Vec<SplitPath> {
    let mut current = Vec::new();
    let mut paths = Vec::new();
    collect_split_paths_inner(root_pane, &mut current, &mut paths);
    paths
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

fn resize_profile_splits(
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
        app.resize_split(path.clone(), ratio);
    }
    split_paths.len()
}

fn rebalance_profile_tab(app: &mut ScratchpadApp) -> usize {
    app.tabs_mut()
        .first_mut()
        .map(rebalance_profile_tab_views)
        .unwrap_or(0)
}

fn rebalance_profile_tab_views(tab: &mut WorkspaceTab) -> usize {
    let _ = tab.rebalance_views_equally();
    let _ = tab.rebalance_views_equally_for_axis(SplitAxis::Horizontal);
    tab.views.len()
}

fn cycle_profile_views(app: &mut ScratchpadApp, view_ids: &[ViewId]) -> usize {
    let mut activations = 0;
    for &view_id in view_ids.iter().skip(1) {
        app.activate_view(view_id);
        activations += 1;
    }
    for &view_id in view_ids.iter().rev().skip(1) {
        app.activate_view(view_id);
        activations += 1;
    }
    activations
}

fn bouncing_indices(count: usize) -> Vec<usize> {
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

fn alternating_axis(index: usize) -> SplitAxis {
    if index.is_multiple_of(2) {
        SplitAxis::Vertical
    } else {
        SplitAxis::Horizontal
    }
}
