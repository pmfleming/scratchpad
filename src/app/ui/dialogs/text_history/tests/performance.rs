//! Reproducible manual fixture: cached UI frames and durable saves, no timing assertions.
use super::*;
use crate::app::services::session_store::{CapturedClosedTab, SessionPersistRequest};
use std::time::Instant;

#[test]
#[ignore = "manual retained Timeline comparison; reports timings without budgets"]
fn profile_retained_history_timeline() {
    for closed in [16, 256] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app(root.path());
        let store = app.state.persistence.session_store.clone();
        let mut live = app.tab_manager.tabs.as_slice().to_vec();
        let mut request = SessionPersistRequest::capture(&live, 0, 16.0, false);
        for index in 0..closed {
            let mut buffer =
                BufferState::new(format!("closed-{index}.txt"), "baseline".into(), None);
            for edit in 0..128 {
                if edit > 0 && edit % 16 == 0 {
                    buffer.navigate_history(2).unwrap();
                }
                buffer
                    .document_mut()
                    .replace_exact_with_history(&format!("{index}-{edit}"));
            }
            buffer.is_dirty = true;
            request
                .closed_tabs
                .push(CapturedClosedTab::capture(&WorkspaceTab::new(buffer), None));
        }
        store.persist_request(request).unwrap();
        let start = Instant::now();
        let page = store
            .timeline_page(None, Vec::new(), String::new())
            .unwrap();
        let first_ms = start.elapsed().as_secs_f64() * 1000.0;
        let first_rows = page.rows.len();
        assert!(first_rows <= 100);
        let start = Instant::now();
        let mut before = page.older;
        let mut pages = 1;
        for _ in 0..5 {
            if let Some(cursor) = before.take() {
                let page = store
                    .timeline_page(Some(cursor), Vec::new(), String::new())
                    .unwrap();
                assert!(page.rows.len() <= 100);
                before = page.older;
                pages += 1;
            }
        }
        let older_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut saves = Vec::new();
        for revision in 0..5 {
            live[0]
                .active_buffer_mut()
                .document_mut()
                .replace_exact_with_history(&format!("live edit-{revision}"));
            live[0].active_buffer_mut().is_dirty = true;
            let start = Instant::now();
            store.persist_profiled(&live, 0, 16.0, false).unwrap();
            saves.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        saves.sort_by(f64::total_cmp);
        app.state.dialogs.text_history.open();
        let mut h = harness(app);
        h.run_steps(3);
        h.get_by_label("Timeline").click();
        for _ in 0..30 {
            h.run_steps(1);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let start = Instant::now();
        h.run_steps(20);
        let frame_us = start.elapsed().as_micros() / 20;
        h.hover_at(egui::pos2(600.0, 500.0));
        let start = Instant::now();
        for _ in 0..20 {
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                source: egui::MouseWheelSource::Wheel,
                phase: egui::TouchPhase::Move,
                delta: egui::vec2(0.0, -60.0),
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
        let scroll_us = start.elapsed().as_micros() / 20;
        let rss = std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find(|l| l.starts_with("VmRSS:"))
            .unwrap()
            .to_owned();
        eprintln!(
            "retained_profile closed={closed} edits={} first_ms={first_ms:.3} first_rows={first_rows} older_ms={older_ms:.3} pages={pages} metadata_queries={} save_median_ms={:.3} frame_us={frame_us} scroll_frame_us={scroll_us} {rss}",
            closed * 128,
            pages * 2,
            saves[2]
        );
    }
}
