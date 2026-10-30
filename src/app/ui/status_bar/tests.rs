use super::{
    CONTROL_CHAR_ICON, HIDDEN_CONTROL_CHAR_ICON, STATUS_PATH_MIN_WIDTH, StatusBarItem,
    StatusBarItemKind, StatusBarPathLayout, artifact_icon, plain_text_icon_color,
    status_attention_color, status_bar_path_layout, status_cursor_range,
};
use crate::app::domain::EditorViewState;
use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};

#[test]
fn zoom_status_is_read_only_tracks_the_active_pane_and_is_hidden_in_settings() {
    use crate::app::{
        app_state::test_support::test_app,
        domain::{BufferState, SplitAxis, WorkspaceTab},
    };
    use eframe::egui;
    use egui_kittest::{Harness, kittest::Queryable};
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    let mut tab = WorkspaceTab::new(BufferState::new("note".into(), "text".into(), None));
    let first = tab.layout.active_view_id();
    let second = tab.split_active_view(SplitAxis::Vertical).unwrap();
    tab.layout.view_mut(first).unwrap().set_zoom_factor(1.25);
    tab.activate_view(first);
    app.tab_manager.set_tabs(vec![tab], 0);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 300.0))
        .with_context_setup(|ctx| {
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
            ctx.set_fonts(fonts);
        })
        .build_ui_state(super::show_status_bar, app);
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(egui::Role::Button, "125%")
            .is_none()
    );
    harness
        .get_by_role_and_label(egui::Role::Label, "125%")
        .click();
    harness.run();
    assert!(!harness.state().state.chrome.showing_settings());
    assert_eq!(
        harness
            .state()
            .tab_manager
            .active_tab()
            .unwrap()
            .layout
            .view(first)
            .unwrap()
            .zoom_factor(),
        1.25
    );
    harness
        .state_mut()
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .activate_view(second);
    harness.run();
    harness.get_by_role_and_label(egui::Role::Label, "100%");
    assert!(harness.query_by_label("125%").is_none());
    crate::app::commands::handle_command(
        harness.state_mut(),
        crate::app::commands::AppCommand::Settings(
            crate::app::commands::SettingsCommand::OpenSettings,
        ),
    );
    harness.run();
    assert!(harness.query_by_label("100%").is_none());
}

#[test]
fn status_prefers_live_cursor_over_pending_cursor() {
    let mut view = EditorViewState::new(1);
    view.cursor_range = Some(CursorRange::two(0, 8));
    view.pending_cursor_range = Some(CursorRange::one(CharCursor::new(3)));

    assert_eq!(status_cursor_range(&view), Some(CursorRange::two(0, 8)));
}

#[test]
fn light_plain_text_icon_is_dark_enough_to_see() {
    let color = plain_text_icon_color(false);

    assert!(color.r() < 80);
    assert!(color.g() < 90);
    assert!(color.b() < 100);
}

#[test]
fn light_status_attention_color_is_readable_on_light_status_bar() {
    let color = status_attention_color(false);

    assert!(color.r() < 180);
    assert!(color.g() < 130);
    assert!(color.b() < 40);
}

#[test]
fn dark_status_attention_color_stays_warm_without_neon_yellow() {
    let color = status_attention_color(true);

    assert!(color.r() >= 220);
    assert!(color.g() >= 170);
    assert!(color.b() >= 60);
    assert!(color.b() < 140);
}

#[test]
fn control_character_status_uses_conventional_marker() {
    assert_eq!(artifact_icon(false, true).0, HIDDEN_CONTROL_CHAR_ICON);
    assert_eq!(artifact_icon(true, true).0, CONTROL_CHAR_ICON);
}

#[test]
fn visible_control_character_status_uses_readable_attention_color() {
    assert_eq!(artifact_icon(true, false).2, status_attention_color(false));
    assert_eq!(artifact_icon(true, true).2, status_attention_color(true));
}

#[test]
fn narrow_status_bar_drops_items_from_the_left() {
    let items = [
        status_item(StatusBarItemKind::LineCount, 40.0),
        status_item(StatusBarItemKind::Cursor, 40.0),
        status_item(StatusBarItemKind::Settings, 40.0),
    ];

    assert_eq!(
        status_bar_path_layout(STATUS_PATH_MIN_WIDTH + 120.0, STATUS_PATH_MIN_WIDTH, &items),
        StatusBarPathLayout {
            visible_start: 0,
            path_width: STATUS_PATH_MIN_WIDTH,
        }
    );
    assert_eq!(
        status_bar_path_layout(STATUS_PATH_MIN_WIDTH + 80.0, STATUS_PATH_MIN_WIDTH, &items),
        StatusBarPathLayout {
            visible_start: 1,
            path_width: STATUS_PATH_MIN_WIDTH,
        }
    );
    assert_eq!(
        status_bar_path_layout(STATUS_PATH_MIN_WIDTH + 40.0, STATUS_PATH_MIN_WIDTH, &items),
        StatusBarPathLayout {
            visible_start: 2,
            path_width: STATUS_PATH_MIN_WIDTH,
        }
    );
}

#[test]
fn path_width_stays_pinned_while_items_disappear() {
    let items = [
        status_item(StatusBarItemKind::LineCount, 40.0),
        status_item(StatusBarItemKind::Cursor, 40.0),
    ];

    assert_eq!(
        status_bar_path_layout(200.0, 100.0, &items),
        StatusBarPathLayout {
            visible_start: 0,
            path_width: 120.0,
        }
    );
    assert_eq!(
        status_bar_path_layout(180.0, 100.0, &items),
        StatusBarPathLayout {
            visible_start: 0,
            path_width: 100.0,
        }
    );
    assert_eq!(
        status_bar_path_layout(160.0, 100.0, &items),
        StatusBarPathLayout {
            visible_start: 1,
            path_width: 100.0,
        }
    );
    assert_eq!(
        status_bar_path_layout(120.0, 100.0, &items),
        StatusBarPathLayout {
            visible_start: 2,
            path_width: 100.0,
        }
    );
}

#[test]
fn path_width_only_shrinks_below_floor_when_no_space_remains() {
    let items = [status_item(StatusBarItemKind::LineCount, 40.0)];

    assert_eq!(
        status_bar_path_layout(80.0, 100.0, &items),
        StatusBarPathLayout {
            visible_start: 1,
            path_width: 80.0,
        }
    );
}

fn status_item(kind: StatusBarItemKind, width: f32) -> StatusBarItem {
    StatusBarItem { kind, width }
}
