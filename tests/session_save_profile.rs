//! Reproducible, opt-in save-cost probe; no timing assertions in ordinary CI.
//! cargo test --release --test session_save_profile -- --ignored --nocapture
use scratchpad::app::domain::{BufferState, WorkspaceTab};
use scratchpad::app::services::session_store::SessionStore;

fn tab(index: usize) -> WorkspaceTab {
    WorkspaceTab::new(BufferState::new(
        format!("note-{index}.txt"),
        format!("unsaved text for tab {index}"),
        None,
    ))
}

#[test]
#[ignore = "10k-tab release profiling; run explicitly on an idle host"]
fn warm_save_costs_at_ten_thousand_tabs() {
    for round in 1..=3 {
        for scenario in ["unchanged", "replace-middle", "close-first", "close-last"] {
            let root = tempfile::tempdir().unwrap();
            let store = SessionStore::new(root.path().to_owned());
            let mut tabs: Vec<_> = (0..10_000).map(tab).collect();
            store.persist(&tabs, 0, 16.0, false).unwrap();
            match scenario {
                "replace-middle" => tabs[5_000] = tab(10_000),
                "close-first" => {
                    tabs.remove(0);
                }
                "close-last" => {
                    tabs.pop();
                }
                _ => {}
            }
            let profile = store.persist_profiled(&tabs, 0, 16.0, false).unwrap();
            println!(
                "round={round} scenario={scenario} total_ms={:.3} capture_ms={:.3} prepare_ms={:.3} content_ms={:.3} rows_ms={:.3} cleanup_ms={:.3}",
                profile.total_ns as f64 / 1e6,
                profile.snapshot_capture_ns as f64 / 1e6,
                profile.manifest_serialize_ns as f64 / 1e6,
                profile.snapshot_write_ns as f64 / 1e6,
                profile.manifest_write_ns as f64 / 1e6,
                profile.stale_cleanup_ns as f64 / 1e6
            );
        }
    }
}
