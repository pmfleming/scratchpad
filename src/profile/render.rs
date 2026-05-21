use super::support::{
    build_balanced_tile_tab, install_profile_tab, plain_text_of_size, sum_profile_iterations,
    unique_profile_session_root,
};
use crate::ScratchpadApp;
use crate::app::app_state::prepare_context_before_first_frame;
use crate::app::capacity_metrics::{
    CapacityMetricsSnapshot, capacity_metrics_snapshot, reset_capacity_metrics,
};
use crate::app::domain::{BufferState, SearchHighlightState};
use crate::app::ui::editor_content::{EditorHighlightStyle, build_layouter};
use eframe::{App, egui};
use std::hint::black_box;
use std::time::Instant;

pub struct UiRenderFrameHarness {
    app: ScratchpadApp,
    ctx: egui::Context,
    frame: eframe::Frame,
    session_root: std::path::PathBuf,
    frame_index: usize,
}

impl UiRenderFrameHarness {
    pub fn new(bytes: usize) -> Self {
        let session_root = unique_profile_session_root("ui-render-frame-harness");
        let session_store =
            crate::app::services::session_store::SessionStore::new(session_root.clone());
        let mut app = ScratchpadApp::with_session_store(session_store);
        app.set_session_persist_on_drop(false);
        install_profile_tab(&mut app, build_balanced_tile_tab(0, 1, bytes), |_| ());
        if let Some(tab) = app.tab_manager.active_tab_mut() {
            tab.layout.set_line_numbers_visible(true);
        }
        let ctx = egui::Context::default();
        prepare_context_before_first_frame(&mut app, &ctx);
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        let frame = eframe::Frame::_new_kittest();
        Self {
            app,
            ctx,
            frame,
            session_root,
            frame_index: 0,
        }
    }

    pub fn run_frame(&mut self) -> u128 {
        self.run_with_input(egui::RawInput::default())
    }

    pub fn run_scroll_frame(&mut self) -> u128 {
        let direction = if (self.frame_index / 30).is_multiple_of(2) {
            -1.0
        } else {
            1.0
        };
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            time: Some(self.frame_index as f64 / 120.0),
            predicted_dt: 1.0 / 120.0,
            ..Default::default()
        };
        input
            .events
            .push(egui::Event::PointerMoved(egui::pos2(640.0, 360.0)));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 96.0 * direction),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        });
        self.run_with_input(input)
    }

    fn run_with_input(&mut self, input: egui::RawInput) -> u128 {
        let started_at = Instant::now();
        let _ = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                App::ui(&mut self.app, ui, &mut self.frame);
            });
        });
        self.frame_index += 1;
        started_at.elapsed().as_nanos()
    }
}

impl Drop for UiRenderFrameHarness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.session_root);
    }
}

pub fn run_ui_render_frame_profile(bytes: usize, iterations: usize) -> u128 {
    let mut harness = UiRenderFrameHarness::new(bytes);
    reset_capacity_metrics();
    (0..iterations).map(|_| harness.run_frame()).sum()
}

pub fn ui_render_frame_metrics(bytes: usize, iterations: usize) -> CapacityMetricsSnapshot {
    let _ = run_ui_render_frame_profile(bytes, iterations);
    capacity_metrics_snapshot()
}

pub fn ui_scroll_frame_metrics(bytes: usize, iterations: usize) -> CapacityMetricsSnapshot {
    let mut harness = UiRenderFrameHarness::new(bytes);
    reset_capacity_metrics();
    for _ in 0..iterations {
        let _ = harness.run_scroll_frame();
    }
    capacity_metrics_snapshot()
}

pub fn run_document_snapshot_profile(bytes: usize, iterations: usize) -> usize {
    let buffer = BufferState::new(
        "document_snapshot_profile.txt".to_owned(),
        plain_text_of_size(bytes),
        None,
    );

    sum_profile_iterations(iterations, || {
        let snapshot = buffer.document_snapshot();
        black_box(snapshot.len_chars() + snapshot.revision() as usize)
    })
}

pub fn run_viewport_extraction_profile(bytes: usize, iterations: usize) -> usize {
    let buffer = BufferState::new(
        "viewport_extraction_profile.txt".to_owned(),
        plain_text_of_size(bytes),
        None,
    );
    let viewport_lines = 48usize;
    let overscan_lines = 12usize;
    let line_step = 17usize;
    let line_count = buffer.line_count.max(1);
    let mut line_start = 0usize;
    let tree = buffer.document().piece_tree().clone();

    sum_profile_iterations(iterations, || {
        let end = (line_start + viewport_lines + overscan_lines).min(line_count);
        let start_char = if line_start < line_count {
            tree.line_info(line_start).start_char
        } else {
            tree.len_chars()
        };
        let end_char = if end < line_count {
            tree.line_info(end).start_char
        } else {
            tree.len_chars()
        };
        let extracted = tree.extract_range(start_char..end_char);

        line_start = if end >= line_count {
            0
        } else {
            (line_start + line_step).min(line_count.saturating_sub(1))
        };

        black_box(extracted.len() + end_char.saturating_sub(start_char))
    })
}

pub fn run_scroll_stress_profile(bytes: usize, iterations: usize) -> usize {
    let buffer = BufferState::new(
        "scroll_stress_profile.txt".to_owned(),
        plain_text_of_size(bytes),
        None,
    );
    let tree = buffer.document().piece_tree().clone();
    let line_count = buffer.line_count.max(1);
    let viewport_lines = 48usize;
    let overscan_lines = 12usize;
    let line_step = 17usize;
    let mut line_start = 0usize;
    let ctx = egui::Context::default();
    let font_id = egui::FontId::monospace(15.0);
    let highlight_style =
        EditorHighlightStyle::new(egui::Color32::from_rgb(90, 146, 214), egui::Color32::WHITE);

    sum_profile_iterations(iterations, || {
        let end = (line_start + viewport_lines + overscan_lines).min(line_count);
        let start_char = if line_start < line_count {
            tree.line_info(line_start).start_char
        } else {
            tree.len_chars()
        };
        let end_char = if end < line_count {
            tree.line_info(end).start_char
        } else {
            tree.len_chars()
        };
        let visible_text = tree.extract_range(start_char..end_char);
        let visible_char_len = visible_text.chars().count();
        let highlight_start = (visible_char_len / 7).max(1);
        let highlight_end = (highlight_start + 48).min(visible_char_len);
        let selection_start = (visible_char_len / 3).max(1);
        let selection_end = (selection_start + 96).min(visible_char_len);
        let mut search_highlights = SearchHighlightState::default();
        if highlight_start < highlight_end {
            search_highlights
                .ranges
                .push(highlight_start..highlight_end);
            search_highlights.active_range_index = Some(0);
        }
        let selection = (selection_start < selection_end).then_some(selection_start..selection_end);

        let mut total_rows = 0usize;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                let mut layouter = build_layouter(
                    font_id.clone(),
                    false,
                    egui::Color32::WHITE,
                    highlight_style,
                    search_highlights.clone(),
                    selection.clone(),
                );

                let galley = layouter(ui, &visible_text, 980.0);
                total_rows += galley.rows.len().max(1);
            });
        });

        line_start = if end >= line_count {
            0
        } else {
            (line_start + line_step).min(line_count.saturating_sub(1))
        };

        total_rows
    })
}

pub fn run_paste_stress_profile(
    base_bytes: usize,
    insert_bytes: usize,
    iterations: usize,
) -> usize {
    let base_text = plain_text_of_size(base_bytes);
    let insert_text = plain_text_of_size(insert_bytes);
    let insert_char_count = insert_text.chars().count();

    let mut buffer = BufferState::new("paste_stress_profile.txt".to_owned(), base_text, None);

    sum_profile_iterations(iterations, || {
        let midpoint = buffer.document().piece_tree().len_chars() / 2;
        let _ = insert_char_count;
        buffer.document_mut().insert_direct(midpoint, &insert_text);
        buffer.refresh_text_metadata();
        black_box(buffer.line_count + buffer.document().piece_tree().len_bytes())
    })
}
