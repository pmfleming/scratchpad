use super::*;
use crate::app::{
    app_state::test_support::test_app,
    commands::{AppCommand, SearchCommand},
    domain::{BufferState, WorkspaceTab},
};
use egui_kittest::{Harness, kittest::Queryable};
use std::time::{Duration, Instant};

pub(super) fn harness(size: egui::Vec2, app: ScratchpadApp) -> Harness<'static, ScratchpadApp> {
    Harness::builder()
        .with_size(size)
        .with_context_setup(|ctx| {
            crate::app::fonts::apply_editor_fonts(
                ctx,
                &crate::app::fonts::EditorFontSelection::scratchpad(
                    crate::app::fonts::EditorFontPreset::Standard,
                ),
                4.0,
            )
            .unwrap();
            ctx.options_mut(|o| o.warn_on_id_clash = true);
        })
        .build_ui_state(
            |ui, app| {
                app.poll_search();
                show_search_strip(ui, app);
            },
            app,
        )
}

#[test]
fn redesigned_fields_and_actions_are_named_and_replace_can_collapse() {
    let root = tempfile::tempdir().unwrap();
    let mut app = crate::app::app_state::test_support::test_app(root.path());
    app.open_search_and_replace();
    let mut harness = harness(egui::vec2(1100.0, 1000.0), app);
    harness.run_steps(3);
    harness.get_by_role_and_label(egui::Role::TextInput, "Find");
    harness.get_by_role_and_label(egui::Role::TextInput, "Replace with");
    harness.get_by_role_and_label(egui::Role::Button, "Replace all included matches");
    harness
        .get_by_role_and_label(egui::Role::Button, "Hide replace")
        .click();
    harness.run_steps(3);
    assert!(
        harness
            .query_by_role_and_label(egui::Role::TextInput, "Replace with")
            .is_none()
    );
}

fn window_rect(harness: &Harness<'static, ScratchpadApp>) -> egui::Rect {
    let layer = widget_ids::area_layer_id(egui::Order::Foreground, "search_window");
    harness
        .ctx
        .memory(|m| m.area_rect(layer.id))
        .expect("search window is shown")
}

fn drag(harness: &mut Harness<'static, ScratchpadApp>, from: egui::Pos2, to: egui::Pos2) {
    for event in [
        egui::Event::PointerMoved(from),
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ] {
        harness.event(event);
        harness.step();
    }
    for step in 1..=8 {
        harness.event(egui::Event::PointerMoved(from.lerp(to, step as f32 / 8.0)));
        harness.step();
    }
    harness.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(3);
}

#[test]
fn resizing_cannot_wrap_or_overlap_the_header() {
    let root = tempfile::tempdir().unwrap();
    let mut app = crate::app::app_state::test_support::test_app(root.path());
    app.open_search_and_replace();
    let mut harness = harness(egui::vec2(1100.0, 900.0), app);
    harness.run_steps(4);
    let before = window_rect(&harness);
    let corner = before.max - egui::vec2(3.0, 3.0);
    drag(
        &mut harness,
        corner,
        egui::pos2(before.left() + 40.0, corner.y),
    );
    let after = window_rect(&harness);
    assert!(
        after.width() < before.width(),
        "the corner drag should shrink the window"
    );

    let title = harness
        .get_by_role_and_label(egui::Role::Label, "Search")
        .rect();
    let mut previous_right = title.right();
    for scope in ["Selection", "File", "Tab", "All tabs"] {
        let rect = harness.get_by_label(scope).rect();
        assert!(
            (rect.center().y - title.center().y).abs() < 4.0,
            "{scope} wrapped: {rect:?} vs title {title:?}"
        );
        assert!(
            rect.left() >= previous_right,
            "{scope} overlaps its neighbour"
        );
        assert!(rect.right() <= after.right(), "{scope} is clipped");
        previous_right = rect.right();
    }
}

pub(super) fn searched_app(root: &std::path::Path, lines: usize) -> ScratchpadApp {
    let mut app = test_app(root);
    let text = (0..lines)
        .map(|i| format!("line {i} has a needle"))
        .collect::<Vec<_>>()
        .join("\n");
    app.tab_manager.tabs.as_mut_slice()[0] =
        WorkspaceTab::new(BufferState::new("needles.txt".into(), text, None));
    for command in [
        SearchCommand::Open,
        SearchCommand::SetSearchQuery {
            query: "needle".into(),
        },
    ] {
        crate::app::commands::handle_command(&mut app, AppCommand::Search(command));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.state.search_state.runtime.searching || app.state.search_state.runtime.dirty {
        assert!(Instant::now() < deadline, "search did not settle");
        app.poll_search();
        std::thread::sleep(Duration::from_millis(5));
    }
    app
}

fn find_focused(harness: &Harness<'static, ScratchpadApp>) -> bool {
    harness
        .get_by_role_and_label(egui::Role::TextInput, "Find")
        .is_focused()
}

fn list_focused(harness: &Harness<'static, ScratchpadApp>) -> bool {
    harness
        .get_by_role_and_label(egui::Role::ListBox, "Search results")
        .is_focused()
}

fn press(
    harness: &mut Harness<'static, ScratchpadApp>,
    modifiers: egui::Modifiers,
    key: egui::Key,
) {
    harness.key_press_modifiers(modifiers, key);
    harness.run_steps(3);
}

#[test]
fn arrows_move_between_find_and_the_results_list() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = harness(egui::vec2(1100.0, 900.0), searched_app(root.path(), 5));
    harness.run_steps(3);
    assert!(find_focused(&harness));

    press(&mut harness, egui::Modifiers::NONE, egui::Key::ArrowDown);
    assert!(list_focused(&harness), "↓ in Find enters the results");
    let first = harness.state().search_active_match_index().unwrap();
    press(&mut harness, egui::Modifiers::NONE, egui::Key::ArrowDown);
    assert_eq!(harness.state().search_active_match_index(), Some(first + 1));
    assert!(list_focused(&harness), "moving keeps focus in the list");
    press(&mut harness, egui::Modifiers::NONE, egui::Key::End);
    assert_eq!(harness.state().search_active_match_index(), Some(4));
    press(&mut harness, egui::Modifiers::NONE, egui::Key::ArrowDown);
    assert_eq!(harness.state().search_active_match_index(), Some(4));
    press(&mut harness, egui::Modifiers::NONE, egui::Key::Home);
    assert_eq!(harness.state().search_active_match_index(), Some(0));
    press(&mut harness, egui::Modifiers::NONE, egui::Key::ArrowUp);
    assert!(find_focused(&harness), "↑ on the first row returns to Find");
}

#[test]
fn enter_in_the_results_list_goes_to_the_editor() {
    let root = tempfile::tempdir().unwrap();
    let mut harness = harness(egui::vec2(1100.0, 900.0), searched_app(root.path(), 5));
    harness.run_steps(3);
    press(&mut harness, egui::Modifiers::NONE, egui::Key::ArrowDown);
    press(&mut harness, egui::Modifiers::NONE, egui::Key::End);
    harness.state_mut().state.focus.clear();
    press(&mut harness, egui::Modifiers::NONE, egui::Key::Enter);
    let app = harness.state();
    let view = app
        .tab_manager
        .active_tab()
        .unwrap()
        .layout
        .active_view_id();
    assert_eq!(app.search_active_match_index(), Some(4));
    assert!(crate::app::app_state::workspace::accessors::should_focus_view(app, view));
}

#[test]
fn alt_arrows_step_through_recent_searches_in_find() {
    use crate::app::app_state::search_state::recent::RecentSearch;
    let root = tempfile::tempdir().unwrap();
    let mut app = searched_app(root.path(), 5);
    app.state.search_state.recent = ["newest", "older"]
        .map(|query| RecentSearch {
            query: query.into(),
            regex: false,
            match_case: false,
            whole_word: false,
        })
        .to_vec();
    let mut harness = harness(egui::vec2(1100.0, 900.0), app);
    harness.run_steps(3);
    press(&mut harness, egui::Modifiers::ALT, egui::Key::ArrowUp);
    assert_eq!(harness.state().state.search_state.query(), "newest");
    press(&mut harness, egui::Modifiers::ALT, egui::Key::ArrowUp);
    assert_eq!(harness.state().state.search_state.query(), "older");
    press(&mut harness, egui::Modifiers::ALT, egui::Key::ArrowDown);
    assert_eq!(harness.state().state.search_state.query(), "newest");
    assert!(find_focused(&harness));
}

#[test]
fn replace_all_review_replaces_the_footer_until_cancelled() {
    use crate::app::services::settings_store::ReviewThreshold;
    let root = tempfile::tempdir().unwrap();
    let mut app = searched_app(root.path(), 5);
    app.state.app_settings.search.review_matches_above = ReviewThreshold::Count(1);
    crate::app::app_state::search_state::api::set_search_replace_open(&mut app, true);
    let mut harness = harness(egui::vec2(1100.0, 900.0), app);
    harness.run_steps(3);
    harness.get_by_label("5 included");
    harness
        .get_by_role_and_label(egui::Role::Button, "Replace all included matches")
        .click();
    harness.run_steps(4);
    harness.get_by_label("Replace 5 matches in 1 file?");
    harness.get_by_role_and_label(egui::Role::Button, "Replace 5 in 1 file");
    assert!(harness.query_by_label_contains("Esc Close").is_none());

    harness
        .get_by_role_and_label(egui::Role::Button, "Cancel")
        .click();
    harness.run_steps(4);
    assert!(
        harness
            .query_by_label("Replace 5 matches in 1 file?")
            .is_none()
    );
    harness.get_by_label_contains("Esc Close");
    assert_eq!(harness.state().state.search_state.query(), "needle");
}
