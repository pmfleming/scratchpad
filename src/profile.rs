mod render;
mod support;

use crate::ScratchpadApp;
use crate::app::app_state::{SearchScope, prepare_context_before_first_frame, search_runtime};
use crate::app::commands::{AppCommand, SearchCommand, WorkspaceCommand};
use eframe::egui;
pub use render::{
    UiRenderFrameHarness, run_document_snapshot_profile, run_paste_stress_profile,
    run_scroll_stress_profile, run_ui_render_frame_profile, run_viewport_extraction_profile,
    ui_render_frame_metrics, ui_scroll_frame_metrics,
};
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;
use support::{
    alternating_axis, bouncing_indices, build_balanced_tile_tab, build_search_current_scope_tab,
    build_view_dense_tab, collect_split_paths, cycle_profile_views, install_navigation_workspace,
    install_profile_tab, install_search_all_tabs, ordered_view_ids, rebalance_profile_tab,
    rebalance_profile_tab_views, resize_profile_splits, sum_profile_iterations,
    target_match_count_for_tab, wait_for_app_state_search_matches, with_isolated_app,
    with_steady_state_app,
};

pub const KB: usize = 1024;
pub const MB: usize = 1024 * KB;
pub const RECOMMENDED_TAB_OPERATION_TABS: usize = 64;
pub const RECOMMENDED_TAB_OPERATION_VIEWS_PER_TAB: usize = 10;
pub const RECOMMENDED_TAB_OPERATION_BYTES_PER_BUFFER: usize = 48 * KB;
pub const RECOMMENDED_TAB_OPERATION_ITERATIONS: usize = 64;
pub const RECOMMENDED_TAB_TILE_COUNT: usize = 16;
pub const RECOMMENDED_TAB_TILE_BYTES: usize = 64 * KB;
pub const RECOMMENDED_TAB_TILE_ITERATIONS: usize = 48;
pub const RECOMMENDED_TAB_STRIP_FRAME_TABS: usize = 10_000;
pub const RECOMMENDED_TAB_STRIP_FRAME_ITERATIONS: usize = 20;
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
pub const RECOMMENDED_UI_RENDER_FRAME_BYTES: usize = 256 * KB;
pub const RECOMMENDED_UI_RENDER_FRAME_ITERATIONS: usize = 120;
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
    run_profile_tab_strip_frame_bin,
    run_tab_strip_frame_profile(
        RECOMMENDED_TAB_STRIP_FRAME_TABS,
        RECOMMENDED_TAB_STRIP_FRAME_ITERATIONS
    ),
    "tab_strip_frame_profile tabs={} iterations={} total_ns={}",
    RECOMMENDED_TAB_STRIP_FRAME_TABS,
    RECOMMENDED_TAB_STRIP_FRAME_ITERATIONS,
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

profile_bin_entry!(
    run_profile_ui_render_frame_bin,
    run_ui_render_frame_profile(
        RECOMMENDED_UI_RENDER_FRAME_BYTES,
        RECOMMENDED_UI_RENDER_FRAME_ITERATIONS
    ),
    "ui_render_frame_profile bytes={} iterations={} total_ns={}",
    RECOMMENDED_UI_RENDER_FRAME_BYTES,
    RECOMMENDED_UI_RENDER_FRAME_ITERATIONS,
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
        let tab_order = bouncing_indices(app.tab_manager.tabs.as_slice().len());

        sum_profile_iterations(iterations, || {
            let mut operations = 0;
            for &index in &tab_order {
                crate::app::commands::handle_command(
                    app,
                    AppCommand::Workspace(WorkspaceCommand::ActivateTab { index }),
                );
                operations += 1;
            }

            if app.tab_manager.tabs.as_slice().len() > 2 {
                let last_index = app.tab_manager.tabs.as_slice().len() - 1;
                crate::app::commands::handle_command(
                    app,
                    AppCommand::Workspace(WorkspaceCommand::ReorderTab {
                        from_index: 1,
                        to_index: last_index,
                    }),
                );
                crate::app::commands::handle_command(
                    app,
                    AppCommand::Workspace(WorkspaceCommand::ReorderTab {
                        from_index: last_index,
                        to_index: 1,
                    }),
                );
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
            |tab| collect_split_paths(&tab.layout.root_pane),
        );
        let mut ratio_phase = false;

        sum_profile_iterations(iterations, || {
            ratio_phase = !ratio_phase;
            resize_profile_splits(app, &split_paths, ratio_phase) + rebalance_profile_tab(app)
        })
    })
}

pub fn run_tab_strip_frame_profile(tab_count: usize, iterations: usize) -> u128 {
    with_steady_state_app("tab-strip-frame", |app| {
        install_navigation_workspace(app, tab_count, 1, KB);
        let ctx = egui::Context::default();
        prepare_context_before_first_frame(app, &ctx);
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 120.0),
            )),
            ..Default::default()
        };
        (0..iterations)
            .map(|_| {
                let started_at = Instant::now();
                let _ = ctx.run_ui(raw_input.clone(), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        crate::app::ui::tab_strip::show_header(ui, app);
                    });
                });
                started_at.elapsed().as_nanos()
            })
            .sum()
    })
}

pub fn run_many_file_lazy_open_profile(paths: &[PathBuf]) -> usize {
    with_isolated_app("many-file-lazy-open", |app| {
        crate::app::services::file_controller::FileController::open_paths_async(
            app,
            paths.to_vec(),
        );
        app.wait_for_background_io_idle();
        app.tab_manager.tabs.as_slice().len() + app.tab_manager.cold_session_tabs().len()
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
            |tab| ordered_view_ids(&tab.layout.root_pane),
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
        let target_match_count = install_profile_tab(
            app,
            build_search_current_scope_tab(file_count, bytes_per_file),
            target_match_count_for_tab,
        );

        run_search_profile_iterations(
            app,
            SearchScope::ActiveWorkspaceTab,
            target_match_count,
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
        let target_match_count = install_search_all_tabs(app, tab_count, bytes_per_tab);
        run_search_profile_iterations(
            app,
            SearchScope::AllOpenTabs,
            target_match_count,
            iterations,
        )
    })
}

pub fn run_search_dispatch_current_profile(
    file_count: usize,
    bytes_per_file: usize,
    iterations: usize,
) -> usize {
    with_isolated_app("search-dispatch-current", |app| {
        app.tab_manager.tabs.as_mut_slice()[0] =
            build_search_current_scope_tab(file_count, bytes_per_file);
        sum_profile_iterations(iterations, || {
            black_box(search_runtime::profile_build_search_request(
                app,
                SearchScope::ActiveWorkspaceTab,
                PROFILE_QUERY,
            ))
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
            black_box(search_runtime::profile_build_search_request(
                app,
                SearchScope::AllOpenTabs,
                PROFILE_QUERY,
            ))
        })
    })
}

pub fn run_split_stress_profile(
    tile_count: usize,
    bytes_per_tile: usize,
    iterations: usize,
) -> usize {
    with_steady_state_app("split-stress", |app| {
        app.tab_manager.tabs.as_mut_slice()[0] =
            build_balanced_tile_tab(0, tile_count, bytes_per_tile);
        let mut axis_seed = 0usize;

        sum_profile_iterations(iterations, || {
            let mut operations = 0usize;
            if let Some(tab) = app.tab_manager.tabs.as_mut_slice().first_mut() {
                let _ = tab.split_active_view(alternating_axis(axis_seed));
                axis_seed = axis_seed.saturating_add(1);
                operations += 1;

                if tab.layout.views.len() > tile_count
                    && let Some(view_id) = tab.layout.views.last().map(|view| view.id)
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
    target_match_count: usize,
    iterations: usize,
) -> usize {
    crate::app::commands::handle_command(app, AppCommand::Search(SearchCommand::Open));
    crate::app::commands::handle_command(
        app,
        AppCommand::Search(SearchCommand::SetSearchScope { scope }),
    );
    crate::app::commands::handle_command(
        app,
        AppCommand::Search(SearchCommand::SetSearchQuery {
            query: PROFILE_RESET_QUERY.to_owned(),
        }),
    );
    wait_for_app_state_search_matches(app, 0);

    sum_profile_iterations(iterations, || {
        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::SetSearchQuery {
                query: PROFILE_QUERY.to_owned(),
            }),
        );
        wait_for_app_state_search_matches(app, target_match_count);
        let match_count = app.state.search_state.match_count();

        crate::app::commands::handle_command(
            app,
            AppCommand::Search(SearchCommand::SetSearchQuery {
                query: PROFILE_RESET_QUERY.to_owned(),
            }),
        );
        wait_for_app_state_search_matches(app, 0);

        match_count
    })
}
