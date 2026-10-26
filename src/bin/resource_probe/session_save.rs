//! Large dirty-buffer autosave workload shared by the performance lens resource probe.
use super::events::{StepDescriptor, StepOutcome, emit_prepared_operation};
use super::{
    BufferState, CharCursor, CursorRange, EditOperation, MB, OperationRecord, PieceSource,
    SessionStore, TextHistoryBudget, WorkspaceTab, unique_probe_root,
};
use std::time::Instant;

fn insert(buffer: &mut BufferState, at: usize, text: &str) {
    let document = buffer.document_mut();
    document.insert_direct_with_source(at, text, PieceSource::Paste);
    document.push_edit_operation_with_source(
        OperationRecord {
            previous_cursor: CursorRange::one(CharCursor::new(at)),
            next_cursor: CursorRange::one(CharCursor::new(at + text.len())),
            edits: vec![EditOperation {
                start_char: at,
                deleted_text: String::new(),
                inserted_text: text.into(),
                deleted_spans: Vec::new(),
            }],
        },
        PieceSource::Paste,
    );
    buffer.is_dirty = true;
}

// Stable, non-repeating ASCII: a repeated line would grossly overstate deduplication.
fn fixture(bytes: usize) -> String {
    let mut state = 0x1234_5678_u64;
    (0..bytes)
        .map(|i| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            if i % 80 == 79 {
                '\n'
            } else {
                (b' ' + (state % 95) as u8) as char
            }
        })
        .collect()
}

pub(super) fn run() {
    let root = unique_probe_root("large-dirty-save");
    std::fs::create_dir_all(&root).expect("create isolated save fixture");
    let mut buffer = BufferState::new("50-MiB-unsaved".into(), fixture(34 * MB), None);
    buffer.document_mut().set_history_budget(TextHistoryBudget {
        per_file_entry_limit: 4096,
        per_file_byte_budget: 32 * MB as u64,
        aggregate_byte_budget: 64 * MB as u64,
        persisted_payload_budget: 16 * MB as u64,
        derived_from_memory: false,
    });
    let payload = fixture(16 * MB);
    for chunk in payload.as_bytes().chunks(16 * 1024) {
        let at = buffer.document().piece_tree().len_chars();
        insert(&mut buffer, at, std::str::from_utf8(chunk).unwrap());
    }
    assert_eq!(buffer.document().operation_undo_depth(), 1024);
    assert_eq!(
        buffer
            .document()
            .exported_history()
            .iter()
            .map(|e| e.payload_bytes())
            .sum::<usize>(),
        16 * MB
    );
    let mut tabs = vec![WorkspaceTab::new(buffer)];
    let store = SessionStore::new(root.clone());
    store
        .persist(&tabs, 0, 16.0, false)
        .expect("seed large session");
    // Initial persistence and fixture construction are excluded. Include capture,
    // serialization, FULL synchronous commit and any automatic checkpoint in saves.
    for step in 0..9 {
        let len = tabs[0].buffers.buffer.document().piece_tree().len_chars();
        let at = match step % 3 {
            0 => 1,
            1 => len / 2,
            _ => len,
        };
        insert(&mut tabs[0].buffers.buffer, at, "x");
        let history = tabs[0].buffers.buffer.document().exported_history();
        let history_payload_bytes = history.iter().map(|e| e.payload_bytes()).sum::<usize>();
        let history_entries = history.len();
        drop(history);
        emit_prepared_operation(
            StepDescriptor {
                scenario: "large_dirty_buffer_save",
                scenario_label: "One-character autosave: 50 MiB buffer, 16 MiB history",
                workload_family: "session-persistence",
                focus: "write-amplification",
                step_index: step,
                workload_value: 50 * MB,
                workload_unit: "bytes",
                workload_label: format!("50 MiB + 16 MiB history; insertion at {at}"),
            },
            serde_json::json!({
                "fixture_revision": 1,
                "buffer_bytes": len + 1,
                "history_payload_bytes": history_payload_bytes,
                "history_entries": history_entries,
                "history_budget_bytes": 16 * MB,
                "inserted_bytes": 1,
                "insert_offset": at,
                "synchronous": "FULL",
                "io_scope": "process write_bytes delta; includes WAL and automatic checkpoints, not physical device traffic",
            }),
            || {
                let start = Instant::now();
                let profile = store
                    .persist_profiled(&tabs, 0, 16.0, false)
                    .expect("autosave");
                let mut outcome =
                    StepOutcome::items_with_manifest(1, Some(profile.manifest_size_bytes));
                outcome.result_label = format!("one-character save at {at}; 1024+ history entries");
                outcome.phase_elapsed_ns = serde_json::json!({
                    "save_ns": start.elapsed().as_nanos(),
                    "snapshot_capture_ns": profile.snapshot_capture_ns,
                    "manifest_serialize_ns": profile.manifest_serialize_ns,
                    "file_write_ns": profile.snapshot_write_ns + profile.manifest_write_ns,
                });
                outcome
            },
        );
    }
    drop(store); // checkpoint-on-close is outside the save samples
    let restored = SessionStore::new(root.clone())
        .load()
        .expect("verify reopened session")
        .expect("session");
    assert_eq!(
        restored.tabs[0].buffers.buffer.text(),
        tabs[0].buffers.buffer.text()
    );
    assert_eq!(
        restored.tabs[0]
            .buffers
            .buffer
            .document()
            .operation_undo_depth(),
        1033
    );
    std::fs::remove_dir_all(root).expect("remove isolated save fixture");
}
