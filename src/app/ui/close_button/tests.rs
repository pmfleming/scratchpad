use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn close_buttons_share_hover_and_activation_in_both_themes_and_sizes() {
    for dark in [false, true] {
        for positioned in [false, true] {
            let mut harness = Harness::builder()
                .with_size(egui::vec2(240.0, 120.0))
                .with_context_setup(move |ctx| {
                    crate::app::app_state::test_support::apply_test_fonts(ctx);
                    ctx.set_visuals(if dark {
                        egui::Visuals::dark()
                    } else {
                        egui::Visuals::light()
                    });
                })
                .build_ui_state(
                    move |ui, clicks: &mut usize| {
                        let button = CloseButton::new("Close example");
                        let response = if positioned {
                            button.show_at(
                                ui,
                                egui::Rect::from_min_size(
                                    egui::pos2(20.0, 20.0),
                                    egui::Vec2::splat(18.0),
                                ),
                                widget_ids::root_id("close-example"),
                            )
                        } else {
                            button
                                .size(egui::vec2(36.0, 30.0))
                                .show(ui, "close-example")
                        };
                        if response.clicked() {
                            *clicks += 1;
                        }
                    },
                    0,
                );
            harness.run();
            let rect = harness.get_by_label("Close example").rect();
            assert!(!has_red_highlight(&harness.output().shapes, rect));
            harness.event(egui::Event::PointerMoved(rect.center()));
            harness.run_steps(2);
            assert!(has_red_highlight(&harness.output().shapes, rect));
            harness.get_by_label("Close example").click();
            harness.run_steps(2);
            assert_eq!(*harness.state(), 1);
            harness.event(egui::Event::PointerMoved(egui::pos2(200.0, 100.0)));
            harness.get_by_label("Close example").focus();
            harness.run_steps(2);
            assert!(!has_red_highlight(&harness.output().shapes, rect));
            harness.key_press(egui::Key::Space);
            harness.run_steps(2);
            assert_eq!(*harness.state(), 2);
        }
    }
}

fn has_red_highlight(shapes: &[egui::epaint::ClippedShape], target: egui::Rect) -> bool {
    shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(rect)
        if rect.rect == target && rect.fill == theme::CLOSE_HOVER_BG
            && rect.corner_radius == egui::CornerRadius::ZERO
            && rect.stroke == egui::Stroke::NONE)
    })
}
