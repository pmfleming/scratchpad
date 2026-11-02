use super::super::super::preview;
use super::{app_with_history, harness};
use crate::app::{domain::CursorRange, theme};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};

#[test]
fn shading_tracks_the_inclusive_preview_range_in_both_directions() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    for (end, text) in [(7, "fourth"), (6, "fifth")] {
        buffer
            .replace_char_ranges_with_undo(
                &[(0..end, text.into())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
    }
    let id = buffer.id;
    let entries: Vec<_> = buffer
        .document()
        .history_entries()
        .iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(entries.len(), 4);
    buffer.undo_last_text_operation().unwrap();
    buffer.undo_last_text_operation().unwrap();
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    let mut colors = [egui::Color32::TRANSPARENT; 2];
    let mut output = h.ctx.run_ui(egui::RawInput::default(), |ui| {
        colors = [
            theme::tab_selected_bg(ui),
            theme::tab_selected_accent(ui).gamma_multiply(0.12),
        ];
    });
    output.textures_delta.clear();
    for destination in [0_usize, 4, 3, 2] {
        let ctx = h.ctx.clone();
        let after = destination.checked_sub(1).map(|i| entries[i]);
        preview::select(&ctx, h.state_mut(), id, after);
        h.run_steps(3);
        let mut rows: Vec<_> = h
            .get_all_by_role(egui::Role::Button)
            .filter(|node| {
                node.accesskit_node()
                    .label()
                    .is_some_and(|label| label.starts_with("State: "))
            })
            .map(|node| node.rect())
            .collect();
        rows.sort_by(|a, b| b.top().total_cmp(&a.top()));
        assert_eq!(rows.len(), 5);
        for (position, rect) in rows.iter().enumerate() {
            let filled = |color| {
                h.output()
                    .shapes
                    .iter()
                    .any(|shape| has_fill(&shape.shape, *rect, color))
            };
            assert_eq!(
                filled(colors[0]),
                position == destination,
                "current {position} to {destination}"
            );
            assert_eq!(
                filled(colors[1]),
                position != destination
                    && (2.min(destination)..=2.max(destination)).contains(&position),
                "range {position} to {destination}"
            );
        }
    }
}

fn has_fill(shape: &egui::Shape, target: egui::Rect, color: egui::Color32) -> bool {
    match shape {
        egui::Shape::Rect(rect) => rect.rect == target && rect.fill == color,
        egui::Shape::Vec(shapes) => shapes.iter().any(|shape| has_fill(shape, target, color)),
        _ => false,
    }
}
