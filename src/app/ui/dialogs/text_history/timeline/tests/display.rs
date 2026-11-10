use super::*;

fn labels(h: &egui_kittest::Harness<'_, (TimelineRow, RowState)>) -> Vec<String> {
    h.output()
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn rows_show_filename_and_contextual_time_with_full_path_on_hover() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let mut live = model::LiveRows::default();
    live.update(&app, "");
    let mut row = live.rows.values().next().unwrap().clone();
    row.path = Some("/home/example/long/path/note.txt".into());
    row.timestamp = 1_600_000_000;
    let time = presentation::clock(row.timestamp);
    let state = RowState {
        previous_time: Some(row.timestamp),
        ..RowState::default()
    };
    let mut h = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .with_size(egui::vec2(440.0, 200.0))
        .build_ui_state(
            |ui, (row, state)| {
                paint_row(ui, row, state);
            },
            (row, state),
        );
    h.run_steps(3);
    let text = labels(&h);
    assert!(text.iter().any(|s| s == "note.txt"));
    assert!(!text.iter().any(|s| s.contains("/home/example")));
    assert!(!text.contains(&time));
    h.hover_at(h.get_by_label("Replace match · note.txt").rect().center());
    h.run_steps(5);
    let text = labels(&h);
    assert!(text.contains(&time));
    assert!(
        text.iter()
            .any(|s| s.contains("/home/example/long/path/note.txt"))
    );
    h.state_mut().1.current = true;
    h.run_steps(3);
    assert!(labels(&h).iter().any(|s| s == "NOW"));
    assert!(!labels(&h).contains(&time));
    assert!(h.query_by_label("Now for note.txt").is_some());
    h.hover_at(egui::pos2(420.0, 180.0));
    h.state_mut().1.current = false;
    h.state_mut().1.previous_time = None;
    h.run_steps(3);
    assert!(labels(&h).contains(&time));
}

#[test]
fn rail_uses_accent_color_and_continues_through_activity_rows() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let event = crate::app::services::session_store::activity::ActivityEvent::new(
        crate::app::services::session_store::activity::ActivityKind::Opened,
        app.tab_manager.active_tab().unwrap().active_buffer(),
    );
    let mut h = egui_kittest::Harness::builder()
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, (row, state, accent)| {
                *accent = theme::tab_selected_accent(ui);
                paint_row(ui, row, state);
            },
            (
                event.timeline_row(),
                RowState::default(),
                egui::Color32::TRANSPARENT,
            ),
        );
    h.run_steps(3);
    let circle = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Circle(c) => Some(c),
            _ => None,
        })
        .unwrap();
    assert_eq!(circle.fill, h.ctx.global_style().visuals.panel_fill);
    assert_eq!(circle.stroke.color, h.state().2);
    let line = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::LineSegment { points, stroke } => {
                assert_eq!(stroke.color, h.state().2);
                Some(points)
            }
            _ => None,
        })
        .unwrap();
    assert!(line[0].y < circle.center.y);
    assert!(line[1].y > circle.center.y);
    h.get_by_label("Opened · note.txt").click();
    h.run_steps(3);
    assert!(h.query_by_label("Now for note.txt").is_none());
}
