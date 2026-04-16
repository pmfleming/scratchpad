use crate::ScratchpadApp;
use crate::app::app_state::SearchScope;
use crate::app::domain::{BufferState, SplitAxis, WorkspaceTab};
use crate::app::services::search::SearchOptions;
use crate::app::services::session_store::SessionStore;
use std::fs;
use std::hint::black_box;
use std::ops::Range;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const KB: usize = 1024;
pub const RECOMMENDED_TAB_OPERATION_TABS: usize = 64;
pub const RECOMMENDED_TAB_OPERATION_ITERATIONS: usize = 16;
pub const RECOMMENDED_TAB_TILE_COUNT: usize = 16;
pub const RECOMMENDED_TAB_TILE_BYTES: usize = 64 * KB;
pub const RECOMMENDED_TAB_TILE_ITERATIONS: usize = 12;
pub const RECOMMENDED_SEARCH_CURRENT_FILES: usize = 16;
pub const RECOMMENDED_SEARCH_CURRENT_BYTES_PER_FILE: usize = 24 * KB;
pub const RECOMMENDED_SEARCH_CURRENT_ITERATIONS: usize = 10;
pub const RECOMMENDED_SEARCH_ALL_TABS: usize = 16;
pub const RECOMMENDED_SEARCH_ALL_BYTES_PER_TAB: usize = 16 * KB;
pub const RECOMMENDED_SEARCH_ALL_ITERATIONS: usize = 10;

const PROFILE_QUERY: &str = "needle";
const PROFILE_RESET_QUERY: &str = "zzzz-no-match";

pub fn run_tab_operations_profile(tab_count: usize, iterations: usize) -> usize {
    let mut total_view_count = 0;
    for _ in 0..iterations {
        total_view_count += black_box(run_tab_operations_cycle(tab_count));
    }
    total_view_count
}

pub fn run_tab_tile_layout_profile(
    tile_count: usize,
    bytes_per_tile: usize,
    iterations: usize,
) -> usize {
    let content = plain_text_of_size(bytes_per_tile);
    let mut total_view_count = 0;

    for _ in 0..iterations {
        let mut tab = build_tile_heavy_tab(tile_count, &content);
        exercise_tile_heavy_tab(&mut tab);
        total_view_count += black_box(tab.views.len());
    }

    total_view_count
}

pub fn run_search_current_app_state_profile(
    file_count: usize,
    bytes_per_file: usize,
    iterations: usize,
) -> usize {
    with_isolated_app("search-current-app-state", |app| {
        let texts = build_scope_texts(file_count, bytes_per_file);
        let expected_matches = full_scan_scope(&texts, PROFILE_QUERY, SearchOptions::default());

        app.tabs_mut()[0].buffer.name = buffer_name_for_index(0);
        app.tabs_mut()[0].buffer.replace_text(texts[0].clone());
        let first_view_id = app.tabs()[0].active_view_id;

        for (item_index, text) in texts.iter().enumerate().skip(1) {
            if item_index.is_multiple_of(2) {
                app.tabs_mut()[0].activate_view(first_view_id);
            }

            app.tabs_mut()[0]
                .open_buffer_as_split(
                    BufferState::new(buffer_name_for_index(item_index), text.clone(), None),
                    if item_index.is_multiple_of(2) {
                        SplitAxis::Horizontal
                    } else {
                        SplitAxis::Vertical
                    },
                    false,
                    0.5,
                )
                .expect("open split buffer");
        }

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
        let texts = build_scope_texts(tab_count, bytes_per_tab);
        let expected_matches = full_scan_scope(&texts, PROFILE_QUERY, SearchOptions::default());

        app.tabs_mut()[0].buffer.name = buffer_name_for_index(0);
        app.tabs_mut()[0].buffer.replace_text(texts[0].clone());

        for (item_index, text) in texts.iter().enumerate().skip(1) {
            app.append_tab(WorkspaceTab::new(BufferState::new(
                buffer_name_for_index(item_index),
                text.clone(),
                None,
            )));
        }

        run_search_profile_iterations(app, SearchScope::AllOpenTabs, expected_matches, iterations)
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

    let mut total_matches = 0;
    for _ in 0..iterations {
        app.set_search_query(PROFILE_QUERY);
        wait_for_app_state_search_matches(app, expected_matches);
        total_matches += black_box(app.search_match_count());

        app.set_search_query(PROFILE_RESET_QUERY);
        wait_for_app_state_search_matches(app, 0);
    }

    total_matches
}

fn with_isolated_app<T>(label: &str, run: impl FnOnce(&mut ScratchpadApp) -> T) -> T {
    let unique_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    let session_root = std::env::temp_dir().join(format!(
        "scratchpad-profile-{label}-{}-{unique_suffix}",
        std::process::id()
    ));
    let session_store = SessionStore::new(session_root.clone());
    let mut app = ScratchpadApp::with_session_store(session_store);
    let result = run(&mut app);
    drop(app);
    let _ = fs::remove_dir_all(&session_root);
    result
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
    let line = "The quick brown fox jumps over the lazy dog 0123456789.\n";
    let repeats = (target_bytes / line.len()).max(1);
    let mut text = String::with_capacity(repeats * line.len());
    for _ in 0..repeats {
        text.push_str(line);
    }
    text
}

fn corpus_text_of_size(item_index: usize, target_bytes: usize) -> String {
    let line = format!(
        "item {item_index} needle alpha beta gamma {}\n",
        "x".repeat(48)
    );
    let repeats = (target_bytes / line.len()).max(1);
    let mut text = String::with_capacity(repeats * line.len());
    for _ in 0..repeats {
        text.push_str(&line);
    }
    text
}

fn build_scope_texts(item_count: usize, bytes_per_item: usize) -> Vec<String> {
    (0..item_count)
        .map(|item_index| corpus_text_of_size(item_index, bytes_per_item))
        .collect()
}

fn buffer_name_for_index(item_index: usize) -> String {
    if item_index.is_multiple_of(2) {
        "mod.rs".to_owned()
    } else {
        "lib.rs".to_owned()
    }
}

fn build_tabs(tab_count: usize) -> Vec<WorkspaceTab> {
    (0..tab_count)
        .map(|index| {
            WorkspaceTab::new(BufferState::new(
                format!("tab_{index}.txt"),
                format!("Content for tab {index}\n{}", "x".repeat(256)),
                None,
            ))
        })
        .collect()
}

fn run_tab_operations_cycle(tab_count: usize) -> usize {
    let mut tabs = build_tabs(tab_count);
    let step_count = tab_count.clamp(8, 64) / 2;

    for step in 0..step_count.max(1) {
        let tab_index = step % tabs.len();
        let axis = if step.is_multiple_of(2) {
            SplitAxis::Vertical
        } else {
            SplitAxis::Horizontal
        };
        let tab = &mut tabs[tab_index];
        tab.split_active_view(axis);

        if tab.views.len() > 1 && step.is_multiple_of(3) {
            let view_id = tab.views[0].id;
            if let Some(promoted) = tab.promote_view_to_new_tab(view_id) {
                tabs.push(promoted);
            }
        }
    }

    if tabs.len() > 2 {
        let target_idx = tabs.len() / 2;
        combine_tabs(&mut tabs, 0, target_idx);
    }

    tabs.iter().map(|tab| tab.views.len()).sum()
}

fn combine_tabs(tabs: &mut Vec<WorkspaceTab>, source_idx: usize, target_idx: usize) {
    if source_idx == target_idx || source_idx >= tabs.len() || target_idx >= tabs.len() {
        return;
    }

    let source_tab = tabs.remove(source_idx);
    let adjusted_target_idx = if source_idx < target_idx {
        target_idx - 1
    } else {
        target_idx
    };
    let target_tab = &mut tabs[adjusted_target_idx];
    let _ = target_tab.combine_with_tab(source_tab, SplitAxis::Horizontal, false, 0.5);
}

fn build_tile_heavy_tab(tile_count: usize, content: &str) -> WorkspaceTab {
    let mut tab = WorkspaceTab::new(BufferState::new(
        "root.txt".to_owned(),
        content.to_owned(),
        None,
    ));

    for index in 1..tile_count {
        let axis = if index.is_multiple_of(2) {
            SplitAxis::Vertical
        } else {
            SplitAxis::Horizontal
        };
        let _ = tab.open_buffer_with_balanced_layout(BufferState::new(
            format!("tile_{index}.txt"),
            content.to_owned(),
            None,
        ));
        let _ = tab.split_active_view(axis);
    }

    tab
}

fn exercise_tile_heavy_tab(tab: &mut WorkspaceTab) {
    let _ = tab.rebalance_views_equally();
    let _ = tab.split_active_view(SplitAxis::Vertical);
    if tab.views.len() > 2 {
        let close_index = tab.views.len() / 3;
        let view_id = tab.views[close_index].id;
        let _ = tab.close_view(view_id);
    }
}

fn full_scan_scope(texts: &[String], query: &str, options: SearchOptions) -> usize {
    texts
        .iter()
        .map(|text| find_matches(text, query, options).len())
        .sum()
}

fn find_matches(text: &str, query: &str, options: SearchOptions) -> Vec<Range<usize>> {
    find_matches_until_limit(text, query, options, usize::MAX)
}

fn find_matches_until_limit(
    text: &str,
    query: &str,
    options: SearchOptions,
    max_results: usize,
) -> Vec<Range<usize>> {
    if query.is_empty() || max_results == 0 {
        return Vec::new();
    }

    let query_char_len = query.chars().count();
    let text_char_len = text.chars().count();
    if query_char_len > text_char_len {
        return Vec::new();
    }

    let char_to_byte = char_to_byte_map(text);
    let folded_query = (!options.match_case).then(|| query.to_lowercase());
    let text_chars = text.chars().collect::<Vec<_>>();
    let mut matches = Vec::new();

    for start in 0..=text_char_len - query_char_len {
        let end = start + query_char_len;
        let candidate = &text[char_to_byte[start]..char_to_byte[end]];
        if !candidate_matches(
            candidate,
            query,
            folded_query.as_deref(),
            options.match_case,
        ) {
            continue;
        }
        if options.whole_word && !is_whole_word_match(&text_chars, start, end) {
            continue;
        }
        matches.push(start..end);
        if matches.len() >= max_results {
            break;
        }
    }

    matches
}

fn candidate_matches(
    candidate: &str,
    query: &str,
    folded_query: Option<&str>,
    match_case: bool,
) -> bool {
    if match_case {
        candidate == query
    } else {
        candidate.to_lowercase() == folded_query.unwrap_or_default()
    }
}

fn char_to_byte_map(text: &str) -> Vec<usize> {
    let mut offsets = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    offsets
}

fn is_whole_word_match(text_chars: &[char], start: usize, end: usize) -> bool {
    let before_is_word = start > 0 && is_word_char(text_chars[start - 1]);
    let after_is_word = end < text_chars.len() && is_word_char(text_chars[end]);
    !before_is_word && !after_is_word
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}
