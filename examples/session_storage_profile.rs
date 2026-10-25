//! Diagnostic phase timings; not a replacement for the performance lens.
use scratchpad::app::domain::{BufferState, WorkspaceTab};
use scratchpad::app::services::session_store::SessionStore;
use std::time::Instant;

fn main() -> std::io::Result<()> {
    let directory = tempfile::tempdir()?;
    let unit = "Scratchpad edits UTF-8: café 東京 Привет مرحبا.\n";
    let mut text = unit.repeat(4096usize.div_ceil(unit.len()));
    let mut end = 4096;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    let tabs: Vec<_> = (0..10_000)
        .map(|index| {
            WorkspaceTab::new(BufferState::new(
                format!("session_{index}.txt"),
                text.clone(),
                None,
            ))
        })
        .collect();
    let store = SessionStore::new(directory.path().to_owned());
    let persist = store.persist_profiled(&tabs, 0, 14.0, true)?;
    println!(
        "{}",
        serde_json::json!({"operation":"initial_persist", "total_ns":persist.total_ns})
    );
    drop(store);
    let store = SessionStore::new(directory.path().to_owned());
    for repetition in 0..4 {
        let start = Instant::now();
        let restored = store.load_profiled()?;
        assert_eq!(restored.profile.tab_count, 10_000);
        println!(
            "{}",
            serde_json::json!({"operation":"full_restore", "repetition":repetition,
            "connection": if repetition == 0 { "fresh" } else { "reused" },
            "os_cache":"unspecified", "total_ns":start.elapsed().as_nanos(),
            "database_read_ns":restored.profile.manifest_read_parse_ns,
            "reconstruction_ns":restored.profile.restore_reconstruction_ns})
        );
    }
    Ok(())
}
