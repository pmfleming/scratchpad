//! Production-path measurement boundaries. Headless rendering ends at tessellation, never present.
use crate::{
    ScratchpadApp,
    app::{
        app_state::SearchScope,
        domain::{BufferState, WorkspaceTab},
    },
};
use eframe::{App, egui};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub(super) fn render_app(app: &mut ScratchpadApp, ctx: &egui::Context) -> usize {
    let mut frame = eframe::Frame::_new_kittest();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 720.0),
        )),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        App::logic(app, ui.ctx(), &mut frame);
        egui::CentralPanel::default().show(ui, |ui| App::ui(app, ui, &mut frame));
    });
    let primitives = ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
    output.textures_delta.clear();
    assert!(
        !primitives.is_empty(),
        "first-render probe produced no render primitives"
    );
    primitives.len()
}

pub fn run_file_first_render_preparation(path: &std::path::Path) -> (u128, usize) {
    super::support::with_isolated_app("file-first-render", |app| {
        let ctx = egui::Context::default();
        crate::app::app_state::prepare_context_before_first_frame(app, &ctx);
        let start = Instant::now();
        crate::app::services::file_controller::FileController::open_paths_async(
            app,
            vec![path.to_path_buf()],
        );
        let primitives = render_opened_file(app, &ctx, path);
        (start.elapsed().as_nanos(), primitives)
    })
}

// Small files arrive asynchronously; a tessellated untitled/loading frame is
// not the first visible file. Poll the production I/O handler until the requested
// document is active, then include its render preparation in the measured interval.
// Staged large files may legitimately render their preview before full hydration.
pub(super) fn render_opened_file(
    app: &mut ScratchpadApp,
    ctx: &egui::Context,
    path: &std::path::Path,
) -> usize {
    let expected = path
        .canonicalize()
        .expect("requested first-visible file exists");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.poll_background_io(ctx);
        if app
            .tab_manager
            .active_tab()
            .is_some_and(|tab| tab.active_buffer().path.as_deref() == Some(expected.as_path()))
        {
            return render_app(app, ctx);
        }
        assert!(
            Instant::now() < deadline,
            "requested file never became visible: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn search_workflow_sample(mode: &str, targets: usize, bytes_per_target: usize) -> Value {
    let setup = Instant::now();
    super::support::with_isolated_app("search-workflow", |app| {
        let text = || {
            format!(
                "{}\nneedle\n",
                "x".repeat(bytes_per_target.saturating_sub(8))
            )
        };
        let tabs = (0..targets)
            .map(|index| {
                WorkspaceTab::new(BufferState::new(
                    format!("target-{index}.txt"),
                    text(),
                    None,
                ))
            })
            .collect::<Vec<_>>();
        if mode == "current" {
            let mut tabs = tabs.into_iter();
            let mut tab = tabs.next().expect("nonempty targets");
            for other in tabs {
                let _ = tab.open_buffer_with_balanced_layout(other.buffers.buffer);
            }
            app.tab_manager.set_tabs(vec![tab], 0);
        } else {
            app.tab_manager.set_tabs(tabs, 0);
        }
        app.open_search();
        app.set_search_scope(match mode {
            "active" => SearchScope::ActiveBuffer,
            "current" => SearchScope::ActiveWorkspaceTab,
            _ => SearchScope::AllOpenTabs,
        });
        let ctx = egui::Context::default();
        crate::app::app_state::prepare_context_before_first_frame(app, &ctx);
        render_app(app, &ctx);
        let layout_before = crate::app::capacity_metrics::capacity_metrics_snapshot();
        let setup_elapsed_ns = setup.elapsed().as_nanos();
        let start = Instant::now();
        app.set_search_query("needle");
        app.poll_search();
        let dispatch_ns = start.elapsed().as_nanos();
        let mut first_result_ns = None;
        let mut first_render_ns = None;
        let mut first_render_layout = Value::Null;
        let deadline = start + Duration::from_secs(30);
        loop {
            app.poll_search();
            if first_result_ns.is_none() && app.search_match_count() > 0 {
                first_result_ns = Some(start.elapsed().as_nanos());
                render_app(app, &ctx);
                first_render_ns = Some(start.elapsed().as_nanos());
                let after = crate::app::capacity_metrics::capacity_metrics_snapshot();
                first_render_layout = json!({
                    "layout_jobs":after.layout_job_count.saturating_sub(layout_before.layout_job_count),
                    "layout_input_bytes":after.layout_input_bytes.saturating_sub(layout_before.layout_input_bytes),
                    "layout_time_ns":after.layout_time_ns.saturating_sub(layout_before.layout_time_ns)});
            }
            if !app.state.search_state.runtime.searching && !app.state.search_state.runtime.dirty {
                break;
            }
            assert!(Instant::now() < deadline, "search workflow timed out");
            std::thread::yield_now();
        }
        let completion_ns = start.elapsed().as_nanos();
        let expected = if mode == "active" { 1 } else { targets };
        assert_eq!(
            app.search_match_count(),
            expected,
            "search correctness must hold before judging latency"
        );
        json!({"setup_elapsed_ns":setup_elapsed_ns,"dispatch":dispatch_ns,
            "first_result":first_result_ns,"first_render":first_render_ns,"completion":completion_ns,
            "match_count":app.search_match_count(), "actual_total_bytes":targets * bytes_per_target,
            "first_render_layout":first_render_layout})
    })
}

/// Headless production paint/input during an injected writer pause. Ends at
/// tessellation; it does not measure GPU present or physical-device I/O latency.
pub fn run_live_text_bulk_profile(bytes: usize) -> Value {
    super::support::with_isolated_app("live-text-bulk", |app| {
        let tab = WorkspaceTab::new(BufferState::new("bulk paste".into(), "base".into(), None));
        app.tab_manager.set_tabs(vec![tab], 0);
        app.apply_history_budget_to_open_buffers();
        crate::app::app_state::workspace::accessors::request_focus_for_active_view(app);
        let ctx = egui::Context::default();
        crate::app::app_state::prepare_context_before_first_frame(app, &ctx);
        render_app(app, &ctx);
        render_app(app, &ctx);
        let unit = "λ🦀 café\n";
        let text = unit.repeat(bytes.div_ceil(unit.len()));
        let expected_bytes = text.len();
        let expected_chars = text.chars().count();
        let store = app.state.persistence.session_store.clone();
        let handle = store.chunk_store_handle().expect("probe store");
        let pause = store
            .profile_pause_writer(Duration::from_millis(200))
            .expect("writer pause");
        let mut frame = eframe::Frame::_new_kittest();
        let started = Instant::now();
        let mut durations = Vec::new();
        let mut paused = Vec::new();
        let mut events = vec![egui::Event::Text("prefix".into()), egui::Event::Paste(text)];
        let mut queued_typing = false;
        let mut saw_preparing = false;
        loop {
            let was_paused = !pause.is_finished();
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 720.0),
                )),
                events: std::mem::take(&mut events),
                ..Default::default()
            };
            let timer = Instant::now();
            App::raw_input_hook(app, &ctx, &mut input);
            assert!(!input.events.iter().any(|event|matches!(event,egui::Event::Paste(text) if text.len()>=crate::app::domain::buffer::BULK_TEXT_BYTES)),"focused editor paste must be moved before egui's begin-pass copy");
            let mut output = ctx.run_ui(input, |ui| {
                App::logic(app, ui.ctx(), &mut frame);
                egui::CentralPanel::default().show(ui, |ui| App::ui(app, ui, &mut frame));
            });
            let _primitives =
                ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
            output.textures_delta.clear();
            let elapsed = timer.elapsed().as_nanos();
            durations.push(elapsed);
            if was_paused {
                paused.push(elapsed);
            }
            let preparing = app
                .tab_manager
                .active_tab()
                .unwrap()
                .active_buffer()
                .is_preparing_text();
            saw_preparing |= preparing;
            if preparing && !queued_typing {
                events.push(egui::Event::Text("t".into()));
                queued_typing = true;
            }
            if saw_preparing && !preparing && queued_typing && pause.is_finished() {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "bulk workflow timed out"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        pause.join().expect("pause task").expect("pause result");
        assert!(saw_preparing && queued_typing);
        let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
        assert_eq!(
            buffer.document().piece_tree().len_chars(),
            expected_chars + 11,
            "paste and input during preparation must both survive"
        );
        let tail = buffer
            .document()
            .piece_tree()
            .extract_range(expected_chars + 6..expected_chars + 11);
        assert_eq!(tail, "tbase");
        assert_eq!(buffer.document().piece_tree().extract_range(0..6), "prefix");
        store.flush_text_chunks().expect("chunk flush");
        durations.sort_unstable();
        paused.sort_unstable();
        json!({"scope":"production logic/input/layout/tessellation; simulated 200 ms writer job", "bytes":expected_bytes,"frames":durations.len(),"frame_p50_ns":durations[durations.len()/2],"frame_p99_ns":durations[(durations.len()*99).div_ceil(100)-1],"frame_max_ns":durations.last(),"writer_paused_frames":paused.len(),"writer_paused_frame_max_ns":paused.last(),"workflow_ns":started.elapsed().as_nanos(),"peak_pending_bytes":handle.peak_pending_bytes(),"pending_flow_threshold_bytes":crate::app::services::chunk_store::BULK_PENDING_BYTES,"queued_typing_preserved":true})
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn production_search_measures_delivery_and_tessellation_with_correct_results() {
        for mode in ["active", "current", "all"] {
            let sample = super::search_workflow_sample(mode, 2, 1024);
            assert!(
                sample["first_result"].as_u64().unwrap()
                    <= sample["first_render"].as_u64().unwrap()
            );
            assert!(
                sample["first_render"].as_u64().unwrap() <= sample["completion"].as_u64().unwrap()
            );
        }
    }
}
