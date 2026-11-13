use super::{
    overflow_drag_hover_ready, overflow_drag_hover_remaining, overflow_popup_target_height,
    show_overflow_button,
};
use crate::app::app_state::{
    ScratchpadApp,
    test_support::{test_app, ui_harness},
};
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::theme::TAB_HEIGHT;
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

#[test]
fn popup_height_tracks_rows_up_to_the_viewport_cap() {
    for (rows, max_rows, expected_rows) in [(4, 20.0, 4.0), (20, 4.5, 4.5), (0, 20.0, 1.0)] {
        assert_eq!(
            overflow_popup_target_height(rows, TAB_HEIGHT * max_rows, TAB_HEIGHT),
            TAB_HEIGHT * expected_rows,
            "{rows} rows"
        );
    }
}

#[test]
fn drag_hover_opens_after_the_dwell_delay_and_repaints_for_the_rest() {
    assert!(!overflow_drag_hover_ready(10.0, 10.39));
    assert!(overflow_drag_hover_ready(10.0, 10.4));
    assert_eq!(
        overflow_drag_hover_remaining(10.0, 10.25),
        Duration::from_millis(150)
    );
    assert_eq!(overflow_drag_hover_remaining(10.0, 11.0), Duration::ZERO);
}

/// Everything the list reported across frames, by slot index.
#[derive(Debug, Default, PartialEq)]
struct Picks {
    activated: Option<usize>,
    closed: Option<usize>,
    settings_activated: bool,
    settings_closed: bool,
}

fn overflow_harness(
    root: &std::path::Path,
) -> (Harness<'static, ScratchpadApp>, Rc<RefCell<Picks>>) {
    let mut app = test_app(root);
    let tabs = ["alpha.txt", "beta.txt", "gamma.txt"]
        .map(|name| WorkspaceTab::new(BufferState::new(name.into(), String::new(), None)));
    app.tab_manager.set_tabs(tabs.into(), 0);
    let picks = Rc::new(RefCell::new(Picks::default()));
    let sink = Rc::clone(&picks);
    let harness = ui_harness(egui::vec2(600.0, 500.0), app, move |ui, app| {
        let ctx = ui.ctx().clone();
        let mut open = app.state.window.overflow_popup_open;
        let outcome = show_overflow_button(&ctx, ui, app, &mut open, &HashMap::new());
        app.state.window.overflow_popup_open = open;
        let mut picks = sink.borrow_mut();
        picks.activated = picks.activated.or(outcome.activated_tab);
        picks.closed = picks.closed.or(outcome.close_requested_tab);
        picks.settings_activated |= outcome.activate_settings;
        picks.settings_closed |= outcome.close_settings;
    });
    (harness, picks)
}

fn open_list(harness: &mut Harness<'static, ScratchpadApp>) {
    harness
        .get_by_label(egui_phosphor::regular::CARET_DOWN)
        .click();
    harness.run_steps(3);
}

fn listed_tabs(harness: &Harness<'static, ScratchpadApp>) -> usize {
    harness.query_all_by_role(egui::Role::Tab).count()
}

fn list_is_open(harness: &Harness<'static, ScratchpadApp>) -> bool {
    harness.state().state.window.overflow_popup_open
}

#[test]
fn typing_filters_rows_and_arrow_enter_opens_the_highlighted_tab() {
    let root = tempfile::tempdir().unwrap();
    let (mut harness, picks) = overflow_harness(root.path());
    assert_eq!(listed_tabs(&harness), 0);
    open_list(&mut harness);
    // Three files plus the open Settings page.
    assert_eq!(listed_tabs(&harness), 4);

    harness.get_by_role(egui::Role::TextInput).type_text("A.T");
    harness.run_steps(2);
    assert_eq!(listed_tabs(&harness), 3);
    harness.get_by_role(egui::Role::TextInput).type_text("zzz");
    harness.run_steps(2);
    assert_eq!(listed_tabs(&harness), 0);
    harness.get_by_label("No tabs");

    for _ in 0..3 {
        harness.key_press(egui::Key::Backspace);
    }
    harness.run_steps(2);
    for key in [
        egui::Key::ArrowDown,
        egui::Key::ArrowDown,
        egui::Key::ArrowDown,
        egui::Key::ArrowUp,
    ] {
        harness.key_press(key);
        harness.run_steps(1);
    }
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);

    assert_eq!(picks.borrow().activated, Some(1));
    assert!(!list_is_open(&harness));
    assert!(harness.query_by_role(egui::Role::TextInput).is_none());
}

#[test]
fn row_clicks_close_or_activate_tabs_and_settings_and_escape_dismisses() {
    let root = tempfile::tempdir().unwrap();
    let (mut harness, picks) = overflow_harness(root.path());
    let clicks = ["Close gamma.txt", "beta.txt", "Close Settings", "Settings"];
    for label in clicks {
        open_list(&mut harness);
        harness.get_by_label(label).click();
        harness.run_steps(3);
        assert!(!list_is_open(&harness), "{label}");
    }
    assert_eq!(
        *picks.borrow(),
        Picks {
            activated: Some(1),
            closed: Some(2),
            settings_activated: true,
            settings_closed: true,
        }
    );

    open_list(&mut harness);
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    assert!(!list_is_open(&harness));
}
