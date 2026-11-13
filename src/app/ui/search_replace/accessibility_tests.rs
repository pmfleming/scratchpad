use super::*;
use egui_kittest::{Harness, kittest::Queryable};

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
