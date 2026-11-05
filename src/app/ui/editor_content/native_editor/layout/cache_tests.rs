use super::*;
use crate::app::ui::editor_content::native_editor::EditorHighlightStyle;

#[test]
fn viewport_preflight_reports_unreadable_text_before_building_a_galley() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source");
    std::fs::write(&path, "viewport λ\r\n".repeat(128)).unwrap();
    let (document, _, line_count) =
        crate::app::domain::TextDocument::from_utf8_file(&path, 0, 0).unwrap();
    let mut buffer = BufferState::new("source".into(), String::new(), Some(path));
    *buffer.document_mut() = document;
    buffer.line_count = line_count;
    buffer
        .document()
        .piece_tree()
        .inject_read_failure_for_test();
    let result = viewport_text_slice(
        &buffer,
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 400.)),
        20.,
        None,
        None,
        false,
    );
    assert!(result.is_err());
    assert!(buffer.document().piece_tree().read_fault().is_some());
    assert_eq!(
        buffer.document().piece_tree().len_bytes(),
        "viewport λ\r\n".repeat(128).len()
    );
}

fn editor_galley(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
) -> Arc<egui::Galley> {
    let font = egui::FontId::monospace(18.0);
    build_editor_galley(
        ui,
        buffer,
        view,
        TextEditOptions::new(
            false,
            false,
            &font,
            egui::Color32::WHITE,
            EditorHighlightStyle::new(egui::Color32::YELLOW, egui::Color32::BLACK),
        ),
        Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 400.0),
        )),
    )
    .unwrap()
    .galley
}

fn input() -> egui::RawInput {
    egui::RawInput {
        max_texture_side: Some(1024),
        ..Default::default()
    }
}

fn fill_font_atlas(ui: &egui::Ui) {
    ui.fonts_mut(|fonts| {
        for size in 40..140 {
            fonts.layout_no_wrap(
                "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".into(),
                egui::FontId::proportional(size as f32),
                egui::Color32::WHITE,
            );
            if fonts.font_atlas_fill_ratio() > 0.8 {
                return;
            }
        }
        panic!("test did not fill the atlas enough to trigger an automatic reset");
    });
}

fn assert_current_atlas(ui: &egui::Ui, galley: &Arc<egui::Galley>) {
    let fresh = ui.fonts_mut(|fonts| fonts.layout_job((*galley.job).clone()));
    assert!(
        **galley == *fresh,
        "cached editor mesh uses stale font-atlas coordinates or metrics"
    );
}

#[test]
fn cached_text_survives_font_atlas_rollover_and_selection_toggling() {
    let ctx = egui::Context::default();
    let mut buffer = BufferState::new(
        "sample".into(),
        "Normal text must remain readable.".into(),
        None,
    );
    let original = buffer.text();
    let mut view = EditorViewState::new(buffer.id);
    let mut output = ctx.run_ui(input(), |ui| {
        let galley = editor_galley(ui, &buffer, &mut view);
        assert_current_atlas(ui, &galley);
        fill_font_atlas(ui);
    });
    output.textures_delta.clear();
    let mut output = ctx.run_ui(input(), |ui| {
        assert!(ui.fonts(|fonts| fonts.font_atlas_fill_ratio()) < 0.8);
        // Another widget allocates glyphs before this pane, so the old UVs now
        // address unrelated glyphs, exactly as after a long session of zooming.
        ui.label("Different glyph allocation order: zyxwvutsrqponmlkjihgfedcba");
        let cached = editor_galley(ui, &buffer, &mut view);
        buffer.active_selection = Some(0..original.chars().count());
        let selected = editor_galley(ui, &buffer, &mut view);
        assert_current_atlas(ui, &selected);
        buffer.active_selection = None;
        let deselected = editor_galley(ui, &buffer, &mut view);
        assert_current_atlas(ui, &cached);
        assert_current_atlas(ui, &deselected);
    });
    output.textures_delta.clear();
    assert_eq!(buffer.text(), original);
}

#[test]
fn cached_text_uses_current_display_scale() {
    let ctx = egui::Context::default();
    let buffer = BufferState::new(
        "sample".into(),
        "Text across display scale changes".into(),
        None,
    );
    let mut view = EditorViewState::new(buffer.id);
    for scale in [1.0, 1.5, 2.0, 1.0] {
        ctx.set_pixels_per_point(scale);
        let mut output = ctx.run_ui(input(), |ui| {
            let galley = editor_galley(ui, &buffer, &mut view);
            assert_current_atlas(ui, &galley);
        });
        output.textures_delta.clear();
    }
}
