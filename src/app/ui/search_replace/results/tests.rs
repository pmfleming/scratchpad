use super::*;
use crate::app::commands::{AppCommand, SearchCommand};
use crate::app::ui::search_replace::accessibility_tests::searched_app;
use egui_kittest::{Harness, kittest::Queryable};

fn group(first_match_index: usize, total_match_count: usize) -> SearchResultGroup {
    SearchResultGroup {
        tab_index: 0,
        buffer_id: first_match_index as u64 + 1,
        buffer_label: format!("file-{first_match_index}"),
        tab_label: "Tab".to_owned(),
        first_match_index,
        total_match_count,
        active: false,
    }
}

#[test]
fn rows_map_back_to_their_file() {
    let groups = [group(0, 3), group(3, 1), group(4, 2)];
    assert_eq!(row_count(&groups), 6);
    assert_eq!(row_count(&[]), 0);
    assert_eq!(group_for(&groups, 0).unwrap().first_match_index, 0);
    assert_eq!(group_for(&groups, 2).unwrap().first_match_index, 0);
    assert_eq!(group_for(&groups, 3).unwrap().first_match_index, 3);
    assert_eq!(group_for(&groups, 5).unwrap().first_match_index, 4);
    assert!(group_for(&groups, 6).is_none());
}

#[test]
fn hit_span_finds_the_match_in_a_trimmed_preview() {
    // "    let needle = needle;" trimmed; the second needle starts at column 18.
    let preview = "let needle = needle;";
    assert_eq!(hit_span(preview, "needle", 18), Some((13, 19)));
    assert_eq!(hit_span(preview, "needle", 9), Some((4, 10)));
    assert_eq!(hit_span("λ needle", "needle", 3), Some((3, 9)));
    assert_eq!(hit_span(preview, "", 1), None);
    assert_eq!(hit_span(preview, "a\nb", 1), None);
    assert_eq!(hit_span(preview, "absent", 1), None);
}

fn list_harness(app: ScratchpadApp) -> Harness<'static, ScratchpadApp> {
    Harness::builder()
        .with_size(egui::vec2(480.0, 400.0))
        .with_context_setup(|ctx| {
            crate::app::fonts::apply_editor_fonts(
                ctx,
                &crate::app::fonts::EditorFontSelection::scratchpad(
                    crate::app::fonts::EditorFontPreset::Standard,
                ),
                4.0,
            )
            .unwrap();
        })
        .build_ui_state(
            |ui, app| {
                let mut state = SearchStripState::from_app(app);
                let mut actions = SearchStripActions::default();
                show_search_results(ui, app, &mut state, &mut actions);
                if let Some(index) = actions.selected_match_index {
                    crate::app::commands::handle_command(
                        app,
                        AppCommand::Search(SearchCommand::ActivateSearchMatch {
                            match_index: index,
                        }),
                    );
                }
            },
            app,
        )
}

#[test]
fn only_visible_rows_are_painted_and_rows_activate_matches() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = list_harness(searched_app(root.path(), 500));
    harness.run_steps(3);
    let rows = harness.query_all_by_role(egui::Role::ListBoxOption).count();
    assert!((1..30).contains(&rows), "painted {rows} of 500 rows");
    harness
        .get_by_label("Line 3 · line 2 has a needle · needles.txt")
        .click();
    harness.run_steps(3);
    assert_eq!(harness.state().search_active_match_index(), Some(2));
}

#[test]
fn shift_clicking_a_dot_toggles_every_match_in_the_file() {
    let root = tempfile::tempdir().unwrap();
    let mut app = searched_app(root.path(), 6);
    crate::app::app_state::search_state::api::set_search_replace_open(&mut app, true);
    let mut harness = list_harness(app);
    harness.run_steps(3);
    harness.get_by_label("Include match 2").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state.search_state.replace.excluded,
        [1].into_iter().collect()
    );
    harness
        .get_by_label("Include match 4")
        .click_modifiers(egui::Modifiers::SHIFT);
    harness.run_steps(2);
    assert_eq!(harness.state().state.search_state.replace.excluded.len(), 6);
}

#[test]
fn space_toggles_the_active_match_and_shift_space_its_file() {
    let root = tempfile::tempdir().unwrap();
    let mut app = searched_app(root.path(), 4);
    crate::app::app_state::search_state::api::set_search_replace_open(&mut app, true);
    crate::app::app_state::search_controller::activate_search_match_at(&mut app, 1);
    let mut harness = list_harness(app);
    harness.run_steps(2);
    harness.ctx.memory_mut(|m| m.request_focus(list_focus_id()));
    harness.run_steps(2);
    harness.key_press(egui::Key::Space);
    harness.run_steps(2);
    assert_eq!(
        harness.state().state.search_state.replace.excluded,
        [1].into_iter().collect()
    );
    // The active match is now excluded, so Shift+Space includes the whole file.
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Space);
    harness.run_steps(2);
    assert!(
        harness
            .state()
            .state
            .search_state
            .replace
            .excluded
            .is_empty()
    );
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Space);
    harness.run_steps(2);
    assert_eq!(harness.state().state.search_state.replace.excluded.len(), 4);
}
