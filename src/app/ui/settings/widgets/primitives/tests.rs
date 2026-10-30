use super::*;
use egui_kittest::{Harness, kittest::Queryable};

fn has_ring(
    shapes: &[egui::epaint::ClippedShape],
    target: egui::Rect,
    color: egui::Color32,
) -> bool {
    fn matches(shape: &egui::Shape, target: egui::Rect, color: egui::Color32) -> bool {
        match shape {
            egui::Shape::Rect(rect) => {
                rect.rect == target && rect.stroke == egui::Stroke::new(2.0, color)
            }
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| matches(shape, target, color)),
            _ => false,
        }
    }
    shapes
        .iter()
        .any(|shape| matches(&shape.shape, target, color))
}

#[test]
fn keyboard_focus_is_painted_for_on_and_off_switches_in_both_themes() {
    for dark in [true, false] {
        for initial in [true, false] {
            let mut harness = Harness::builder()
                .with_size(egui::vec2(400.0, 220.0))
                .with_context_setup(move |ctx| {
                    ctx.set_visuals(if dark {
                        egui::Visuals::dark()
                    } else {
                        egui::Visuals::light()
                    });
                })
                .build_ui_state(
                    |ui, state: &mut (bool, bool, egui::Color32)| {
                        SettingsUi::apply_typography(ui);
                        SettingsUi::apply_controls(ui);
                        state.2 = ui.visuals().selection.stroke.color;
                        let _ = ui.button("Before");
                        ui.add_enabled_ui(state.1, |ui| {
                            ui.name_inputs("Example switch", |ui| {
                                // A bounded row avoids inheriting the full remaining page height.
                                ui.allocate_ui(egui::vec2(100.0, 36.0), |ui| {
                                    toggle_switch(ui, "example", &mut state.0);
                                });
                            });
                        });
                        let _ = ui.button("After");
                    },
                    (initial, true, egui::Color32::TRANSPARENT),
                );
            harness.run();
            let target = harness.get_by_role(egui::Role::Switch).rect();
            assert!(target.height() >= 36.0);
            let ring =
                egui::Rect::from_center_size(target.center(), egui::vec2(42.0, 22.0)).expand(3.0);
            let accent = harness.state().2;
            assert!(!has_ring(&harness.output().shapes, ring, accent));
            harness.get_by_label("Before").focus();
            harness.run_steps(2);
            harness.key_press(egui::Key::Tab);
            harness.run_steps(2);
            assert!(harness.get_by_role(egui::Role::Switch).is_focused());
            assert!(has_ring(&harness.output().shapes, ring, accent));
            assert_eq!(harness.state().0, initial, "focus must not toggle");
            harness.key_press(egui::Key::Space);
            harness.run_steps(2);
            assert_eq!(harness.state().0, !initial);
            assert!(has_ring(&harness.output().shapes, ring, accent));
            harness.key_press(egui::Key::Tab);
            harness.run_steps(2);
            assert!(harness.get_by_label("After").is_focused());
            assert!(!has_ring(&harness.output().shapes, ring, accent));
            harness.state_mut().1 = false;
            harness.run_steps(2);
            harness.get_by_role(egui::Role::Switch).focus();
            harness.key_press(egui::Key::Space);
            harness.run_steps(2);
            assert_eq!(harness.state().0, !initial);
            assert!(!has_ring(&harness.output().shapes, ring, accent));
        }
    }
}
