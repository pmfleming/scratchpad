mod support;

use crate::ScratchpadApp;
use crate::app::app_state::SearchScope;
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferState, SearchHighlightState};
use crate::app::ui::editor_content::{EditorHighlightStyle, build_layouter};
use eframe::egui;
use std::hint::black_box;
use support::*;

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

pub(super) const PROFILE_QUERY: &str = "needle";
const PROFILE_RESET_QUERY: &str = "zzzz-no-match";
pub(super) const SEARCH_VIEW_DUPLICATES_PER_TAB: usize = 4;

macro_rules! profile_bin_entry {
    ($entry:ident, $runner:ident($($run_arg:expr),+), $format:literal, $($print_arg:expr),+ $(,)?) => {
        pub fn $entry() {
            let total = black_box($runner($($run_arg),+));
            println!($format, $($print_arg,)+ total);
        }
    };
}

profile_bin_entry!(
    run_profile_document_snapshot_bin,
    run_document_snapshot_profile(
        RECOMMENDED_DOCUMENT_SNAPSHOT_BYTES,
        RECOMMENDED_DOCUMENT_SNAPSHOT_ITERATIONS
    ),
    "document_snapshot_profile bytes={} iterations={} total={}",
    RECOMMENDED_DOCUMENT_SNAPSHOT_BYTES,
    RECOMMENDED_DOCUMENT_SNAPSHOT_ITERATIONS,
);

profile_bin_entry!(
    run_profile_paste_stress_bin,
    run_paste_stress_profile(
        RECOMMENDED_PASTE_STRESS_BASE_BYTES,
        RECOMMENDED_PASTE_STRESS_INSERT_BYTES,
        RECOMMENDED_PASTE_STRESS_ITERATIONS
    ),
    "paste_stress_profile base_bytes={} insert_bytes={} iterations={} total_work={}",
    RECOMMENDED_PASTE_STRESS_BASE_BYTES,
    RECOMMENDED_PASTE_STRESS_INSERT_BYTES,
    RECOMMENDED_PASTE_STRESS_ITERATIONS,
);

profile_bin_entry!(
    run_profile_scroll_stress_bin,
    run_scroll_stress_profile(
        RECOMMENDED_SCROLL_STRESS_BYTES,
        RECOMMENDED_SCROLL_STRESS_ITERATIONS
    ),
    "scroll_stress_profile bytes={} iterations={} total_rows={}",
    RECOMMENDED_SCROLL_STRESS_BYTES,
    RECOMMENDED_SCROLL_STRESS_ITERATIONS,
);

profile_bin_entry!(
    run_profile_search_all_tabs_bin,
    run_search_all_tabs_profile(
        RECOMMENDED_SEARCH_ALL_TABS,
        RECOMMENDED_SEARCH_ALL_BYTES_PER_TAB,
        RECOMMENDED_SEARCH_ALL_ITERATIONS
    ),
    "search_all_tabs_profile tabs={} bytes_per_tab={} iterations={} total_matches={}",
    RECOMMENDED_SEARCH_ALL_TABS,
    RECOMMENDED_SEARCH_ALL_BYTES_PER_TAB,
    RECOMMENDED_SEARCH_ALL_ITERATIONS,
);

profile_bin_entry!(
    run_profile_search_current_app_state_bin,
    run_search_current_app_state_profile(
        RECOMMENDED_SEARCH_CURRENT_FILES,
        RECOMMENDED_SEARCH_CURRENT_BYTES_PER_FILE,
        RECOMMENDED_SEARCH_CURRENT_ITERATIONS
    ),
    "search_current_app_state_profile files={} bytes_per_file={} iterations={} total_matches={}",
    RECOMMENDED_SEARCH_CURRENT_FILES,
    RECOMMENDED_SEARCH_CURRENT_BYTES_PER_FILE,
    RECOMMENDED_SEARCH_CURRENT_ITERATIONS,
);

profile_bin_entry!(
    run_profile_split_stress_bin,
    run_split_stress_profile(
        RECOMMENDED_SPLIT_STRESS_TILES,
        RECOMMENDED_SPLIT_STRESS_BYTES_PER_TILE,
        RECOMMENDED_SPLIT_STRESS_ITERATIONS
    ),
    "split_stress_profile tiles={} bytes_per_tile={} iterations={} total_actions={}",
    RECOMMENDED_SPLIT_STRESS_TILES,
    RECOMMENDED_SPLIT_STRESS_BYTES_PER_TILE,
    RECOMMENDED_SPLIT_STRESS_ITERATIONS,
);

profile_bin_entry!(
    run_profile_tab_operations_bin,
    run_tab_operations_profile(
        RECOMMENDED_TAB_OPERATION_TABS,
        RECOMMENDED_TAB_OPERATION_ITERATIONS
    ),
    "tab_operations_profile tabs={} views_per_tab={} bytes_per_buffer={} iterations={} total_actions={}",
    RECOMMENDED_TAB_OPERATION_TABS,
    RECOMMENDED_TAB_OPERATION_VIEWS_PER_TAB,
    RECOMMENDED_TAB_OPERATION_BYTES_PER_BUFFER,
    RECOMMENDED_TAB_OPERATION_ITERATIONS,
);

profile_bin_entry!(
    run_profile_tab_tile_layout_bin,
    run_tab_tile_layout_profile(
        RECOMMENDED_TAB_TILE_COUNT,
        RECOMMENDED_TAB_TILE_BYTES,
        RECOMMENDED_TAB_TILE_ITERATIONS
    ),
    "tab_tile_layout_profile tiles={} bytes_per_tile={} iterations={} total_actions={}",
    RECOMMENDED_TAB_TILE_COUNT,
    RECOMMENDED_TAB_TILE_BYTES,
    RECOMMENDED_TAB_TILE_ITERATIONS,
);

profile_bin_entry!(
    run_profile_view_navigation_bin,
    run_view_navigation_profile(
        RECOMMENDED_VIEW_NAVIGATION_VIEWS,
        RECOMMENDED_VIEW_NAVIGATION_BYTES_PER_BUFFER,
        RECOMMENDED_VIEW_NAVIGATION_ITERATIONS
    ),
    "view_navigation_profile views={} bytes_per_buffer={} iterations={} total_activations={}",
    RECOMMENDED_VIEW_NAVIGATION_VIEWS,
    RECOMMENDED_VIEW_NAVIGATION_BYTES_PER_BUFFER,
    RECOMMENDED_VIEW_NAVIGATION_ITERATIONS,
);

profile_bin_entry!(
    run_profile_viewport_extraction_bin,
    run_viewport_extraction_profile(
        RECOMMENDED_VIEWPORT_EXTRACTION_BYTES,
        RECOMMENDED_VIEWPORT_EXTRACTION_ITERATIONS
    ),
    "viewport_extraction_profile bytes={} iterations={} total={}",
    RECOMMENDED_VIEWPORT_EXTRACTION_BYTES,
    RECOMMENDED_VIEWPORT_EXTRACTION_ITERATIONS,
);

pub fn run_profile_search_dispatch_bin() {
    let current_total = black_box(run_search_dispatch_current_profile(
        RECOMMENDED_SEARCH_DISPATCH_CURRENT_FILES,
        RECOMMENDED_SEARCH_DISPATCH_BYTES_PER_ITEM,
        RECOMMENDED_SEARCH_DISPATCH_ITERATIONS,
    ));
    let all_total = black_box(run_search_dispatch_all_tabs_profile(
        RECOMMENDED_SEARCH_DISPATCH_ALL_TABS,
        RECOMMENDED_SEARCH_DISPATCH_BYTES_PER_ITEM,
        RECOMMENDED_SEARCH_DISPATCH_ITERATIONS,
    ));
    println!(
        "search_dispatch_profile current_files={} all_tabs={} bytes_per_item={} iterations={} current_total={} all_total={}",
        RECOMMENDED_SEARCH_DISPATCH_CURRENT_FILES,
        RECOMMENDED_SEARCH_DISPATCH_ALL_TABS,
        RECOMMENDED_SEARCH_DISPATCH_BYTES_PER_ITEM,
        RECOMMENDED_SEARCH_DISPATCH_ITERATIONS,
        current_total,
        all_total
    );
}

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
        let split_paths = install_profile_tab(
            app,
            build_balanced_tile_tab(0, tile_count, bytes_per_tile),
            |tab| collect_split_paths(&tab.root_pane),
        );
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
        let view_ids = install_profile_tab(
            app,
            build_view_dense_tab(0, view_count, bytes_per_buffer),
            |tab| ordered_view_ids(&tab.root_pane),
        );

        sum_profile_iterations(iterations, || cycle_profile_views(app, &view_ids))
    })
}

pub fn run_search_current_app_state_profile(
    file_count: usize,
    bytes_per_file: usize,
    iterations: usize,
) -> usize {
    with_isolated_app("search-current-app-state", |app| {
        let expected_matches = install_profile_tab(
            app,
            build_search_current_scope_tab(file_count, bytes_per_file),
            expected_matches_for_tab,
        );

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
        let expected_matches = install_search_all_tabs(app, tab_count, bytes_per_tab);
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
        let _ = install_search_all_tabs(app, tab_count, bytes_per_tab);

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
    let tree = buffer.document().piece_tree().clone();

    sum_profile_iterations(iterations, || {
        let end = (line_start + viewport_lines + overscan_lines).min(line_count);
        let start_char = if line_start < line_count {
            tree.line_info(line_start).start_char
        } else {
            tree.len_chars()
        };
        let end_char = if end < line_count {
            tree.line_info(end).start_char
        } else {
            tree.len_chars()
        };
        let extracted = tree.extract_range(start_char..end_char);

        line_start = if end >= line_count {
            0
        } else {
            (line_start + line_step).min(line_count.saturating_sub(1))
        };

        black_box(extracted.len() + end_char.saturating_sub(start_char))
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
                    total_rows += galley.rows.len().max(1);
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
