mod support;

use crate::ScratchpadApp;
use crate::app::app_state::{SearchScope, prepare_context_before_first_frame, search_runtime};
use crate::app::capacity_metrics::{capacity_metrics_snapshot, reset_capacity_metrics};
use crate::app::commands::{AppCommand, SearchCommand, WorkspaceCommand};
use crate::app::domain::{BufferState, SearchHighlightState};
use crate::app::ui::editor_content::{EditorHighlightStyle, build_layouter};
use eframe::{App, egui};
use std::hint::black_box;
use std::time::Instant;
use support::{
    alternating_axis, bouncing_indices, build_balanced_tile_tab, build_search_current_scope_tab,
    build_view_dense_tab, collect_split_paths, cycle_profile_views, expected_matches_for_tab,
    install_navigation_workspace, install_profile_tab, install_search_all_tabs, ordered_view_ids,
    plain_text_of_size, rebalance_profile_tab, rebalance_profile_tab_views, resize_profile_splits,
    sum_profile_iterations, unique_profile_session_root, wait_for_app_state_search_matches,
    with_isolated_app, with_steady_state_app,
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

pub struct UiRenderFrameHarness {
    app: ScratchpadApp,
    ctx: egui::Context,
    frame: eframe::Frame,
    session_root: std::path::PathBuf,
    frame_index: usize,
}

impl UiRenderFrameHarness {
    pub fn new(bytes: usize) -> Self {
        let session_root = unique_profile_session_root("ui-render-frame-harness");
        let session_store =
            crate::app::services::session_store::SessionStore::new(session_root.clone());
        let mut app = ScratchpadApp::with_session_store(session_store);
        app.set_session_persist_on_drop(false);
        install_profile_tab(&mut app, build_balanced_tile_tab(0, 1, bytes), |_| ());
        if let Some(tab) = app.tab_manager.active_tab_mut() {
            tab.set_line_numbers_visible(true);
        }
        let ctx = egui::Context::default();
        prepare_context_before_first_frame(&mut app, &ctx);
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        let frame = eframe::Frame::_new_kittest();
        Self {
            app,
            ctx,
            frame,
            session_root,
            frame_index: 0,
        }
    }

    pub fn run_frame(&mut self) -> u128 {
        self.run_with_input(egui::RawInput::default())
    }

    pub fn run_scroll_frame(&mut self) -> u128 {
        let direction = if (self.frame_index / 30).is_multiple_of(2) {
            -1.0
        } else {
            1.0
        };
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            time: Some(self.frame_index as f64 / 120.0),
            predicted_dt: 1.0 / 120.0,
            ..Default::default()
        };
        input
            .events
            .push(egui::Event::PointerMoved(egui::pos2(640.0, 360.0)));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 96.0 * direction),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        });
        self.run_with_input(input)
    }

    fn run_with_input(&mut self, input: egui::RawInput) -> u128 {
        let started_at = Instant::now();
        #[allow(deprecated)]
        let _ = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                App::ui(&mut self.app, ui, &mut self.frame);
            });
        });
        self.frame_index += 1;
        started_at.elapsed().as_nanos()
    }
}

impl Drop for UiRenderFrameHarness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.session_root);
    }
}

pub fn run_ui_render_frame_profile(bytes: usize, iterations: usize) -> u128 {
    let mut harness = UiRenderFrameHarness::new(bytes);
    reset_capacity_metrics();
    (0..iterations).map(|_| harness.run_frame()).sum()
}

pub fn ui_render_frame_metrics(
    bytes: usize,
    iterations: usize,
) -> crate::app::capacity_metrics::CapacityMetricsSnapshot {
    let _ = run_ui_render_frame_profile(bytes, iterations);
    capacity_metrics_snapshot()
}

pub fn ui_scroll_frame_metrics(
    bytes: usize,
    iterations: usize,
) -> crate::app::capacity_metrics::CapacityMetricsSnapshot {
    let mut harness = UiRenderFrameHarness::new(bytes);
    reset_capacity_metrics();
    for _ in 0..iterations {
        let _ = harness.run_scroll_frame();
    }
    capacity_metrics_snapshot()
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
                app.handle_command(AppCommand::Workspace(WorkspaceCommand::ActivateTab {
                    index,
                }));
                operations += 1;
            }

            if app.tab_manager.tabs.as_slice().len() > 2 {
                let last_index = app.tab_manager.tabs.as_slice().len() - 1;
                app.handle_command(AppCommand::Workspace(WorkspaceCommand::ReorderTab {
                    from_index: 1,
                    to_index: last_index,
                }));
                app.handle_command(AppCommand::Workspace(WorkspaceCommand::ReorderTab {
                    from_index: last_index,
                    to_index: 1,
                }));
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
    let buffer = BufferState::new(
        "scroll_stress_profile.txt".to_owned(),
        plain_text_of_size(bytes),
        None,
    );
    let tree = buffer.document().piece_tree().clone();
    let line_count = buffer.line_count.max(1);
    let viewport_lines = 48usize;
    let overscan_lines = 12usize;
    let line_step = 17usize;
    let mut line_start = 0usize;
    let ctx = egui::Context::default();
    let font_id = egui::FontId::monospace(15.0);
    let highlight_style =
        EditorHighlightStyle::new(egui::Color32::from_rgb(90, 146, 214), egui::Color32::WHITE);

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
        let visible_text = tree.extract_range(start_char..end_char);
        let visible_char_len = visible_text.chars().count();
        let highlight_start = (visible_char_len / 7).max(1);
        let highlight_end = (highlight_start + 48).min(visible_char_len);
        let selection_start = (visible_char_len / 3).max(1);
        let selection_end = (selection_start + 96).min(visible_char_len);
        let mut search_highlights = SearchHighlightState::default();
        if highlight_start < highlight_end {
            search_highlights
                .ranges
                .push(highlight_start..highlight_end);
            search_highlights.active_range_index = Some(0);
        }
        let selection = (selection_start < selection_end).then_some(selection_start..selection_end);

        let mut total_rows = 0usize;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                let mut layouter = build_layouter(
                    font_id.clone(),
                    false,
                    egui::Color32::WHITE,
                    highlight_style,
                    search_highlights.clone(),
                    selection.clone(),
                );

                let galley = layouter(ui, &visible_text, 980.0);
                total_rows += galley.rows.len().max(1);
            });
        });

        line_start = if end >= line_count {
            0
        } else {
            (line_start + line_step).min(line_count.saturating_sub(1))
        };

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
    expected_matches: usize,
    iterations: usize,
) -> usize {
    app.handle_command(AppCommand::Search(SearchCommand::Open));
    app.handle_command(AppCommand::Search(SearchCommand::SetSearchScope { scope }));
    app.handle_command(AppCommand::Search(SearchCommand::SetSearchQuery {
        query: PROFILE_RESET_QUERY.to_owned(),
    }));
    wait_for_app_state_search_matches(app, 0);

    sum_profile_iterations(iterations, || {
        app.handle_command(AppCommand::Search(SearchCommand::SetSearchQuery {
            query: PROFILE_QUERY.to_owned(),
        }));
        wait_for_app_state_search_matches(app, expected_matches);
        let match_count = app.state.search_state.match_count();

        app.handle_command(AppCommand::Search(SearchCommand::SetSearchQuery {
            query: PROFILE_RESET_QUERY.to_owned(),
        }));
        wait_for_app_state_search_matches(app, 0);

        match_count
    })
}
