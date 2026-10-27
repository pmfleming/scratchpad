use super::{ScratchpadApp, category_heading, egui, expandable_card};
use std::time::Duration;

pub(super) fn render_recovery_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "History & Recovery");
    super::sections::text_history_budget_card(ui, app);
    ui.add_space(super::SettingsUi::LAYOUT.card_gap);
    expandable_card(
        ui,
        "recovery.policy",
        egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
        "Automatic recovery",
        "Retention and reopening policy.",
        false,
        |ui| {
            let mut policy = app.state.app_settings.history.recovery.clone();
            super::sections::byte_budget_row(
                ui,
                "Recovery points",
                "Global logical snapshot budget (shared references count toward each snapshot).",
                &mut policy.snapshot_byte_budget,
                1,
                16384,
            );
            super::sections::byte_budget_row(
                ui,
                "Closed archives",
                "Logical archive budget; protected archives may exceed it.",
                &mut policy.archive_byte_budget,
                1,
                65536,
            );
            ui.horizontal(|ui| {
                let label = ui.label("Archive retention (days; 0 = no age expiry)");
                ui.add(egui::DragValue::new(&mut policy.archive_retention_days).range(0..=36500))
                    .labelled_by(label.id);
            });
            ui.checkbox(
                &mut policy.reopen_archives,
                "Restore archived tabs when reopening files",
            );
            ui.checkbox(
                &mut policy.open_changed_disk_files,
                "Also open changed disk files beside recovered unsaved text",
            );
            ui.label("Recent edits are undoable. Older states become snapshot-only recovery points. When budgets are full, older unprotected recovery data expires automatically.");
            ui.label("Changed editing snapshots are sampled at most once every five minutes. Undo compaction preserves an earlier boundary independently of that interval. One pending boundary per buffer is held in memory until a successful session save; it is separate from the detailed undo budget.");
            ui.label("Archives are compacted first, then older interior snapshots are thinned, then the oldest eligible archives expire. Cleanup runs on saves and at least once per minute while the app is running. Archived layouts take precedence over Open Here.");
            ui.label("External changes: clean archives open current disk text and retain the archived history. Conflicting unsaved text recovers in a separate untitled tab; the setting above controls whether to also open disk text. Missing files recover as untitled tabs. Original files are never overwritten automatically.");
            ui.label("Pinned archives never expire automatically. They may exceed both recovery budgets; unpin them in Recently Closed to allow cleanup. Reopening transfers history into the live tab and ends that archive's pin.");
            ui.label("Budgets count logical payloads, not database allocation. Shared text references count conservatively toward retention limits. Indexes, reusable pages and the write-ahead log affect actual disk usage. An oversized unpinned archive or recovery point can expire immediately.");
            if policy != app.state.app_settings.history.recovery {
                crate::app::app_state::settings_controller::set_recovery_settings(app, policy);
            }
        },
    );
    ui.add_space(super::SettingsUi::LAYOUT.card_gap);
    expandable_card(
        ui,
        "recovery.storage",
        egui_phosphor::regular::FLOPPY_DISK,
        "Disk usage",
        "Actual data-directory usage and stored recovery data.",
        false,
        |ui| {
            render_usage(ui, app);
        },
    );
}

fn render_usage(ui: &mut egui::Ui, app: &ScratchpadApp) {
    let store = app.state.persistence.session_store.clone();
    let settings_root = crate::app::app_state::settings_state::settings_path(app)
        .parent()
        .unwrap_or(store.root())
        .to_owned();
    let result = crate::app::ui::recovery::query_with_interval(
        ui,
        ("storage", store.root().to_owned(), settings_root.clone()),
        Duration::from_secs(5),
        move || store.storage_usage_with_settings(&settings_root),
    );
    ui.label(format!(
        "Data directory: {}",
        app.state.persistence.session_store.root().display()
    ));
    match result {
        None => {
            ui.label("Measuring disk usage…");
        }
        Some(Err(error)) => {
            ui.label(format!("Could not measure disk usage: {error}"));
        }
        Some(Ok(usage)) => {
            for (label, bytes) in [
                ("Total Scratchpad data files", usage.total_file_bytes),
                ("Session database (all recovery data)", usage.database),
                ("Write-ahead log / shared memory", usage.journal),
                ("Other state-directory files", usage.other),
                ("Separate settings-directory files", usage.settings),
            ] {
                usage_row(ui, label, bytes);
            }
            if let Some(bytes) = usage.allocated_bytes {
                usage_row(ui, "Total file allocation on disk", bytes);
            }
            if let Some(error) = &usage.stored_error {
                ui.label(format!("Database breakdown unavailable: {error}"));
                return;
            }
            usage_row(
                ui,
                "Reusable space inside database",
                usage.stored.reusable_pages,
            );
            ui.separator();
            ui.label("Stored payload breakdown (not exclusive disk allocation)");
            for (label, bytes) in [
                ("Session metadata", usage.stored.session_metadata),
                ("Undo data (open and archived)", usage.stored.undo),
                ("Closed archive metadata", usage.stored.archive_metadata),
                (
                    "Recovery point metadata (estimate)",
                    usage.stored.snapshot_metadata,
                ),
                ("Current / archived text only", usage.stored.live_text),
                ("Historical snapshot text only", usage.stored.recovery_text),
                ("Text shared by both", usage.stored.shared_text),
                (
                    "Unreferenced text awaiting cleanup",
                    usage.stored.unreferenced_text,
                ),
            ] {
                usage_row(ui, label, bytes);
            }
            ui.separator();
            ui.label(format!(
                "{} closed archives · {} recovery points",
                usage.stored.archives, usage.stored.recovery_points
            ));
            usage_row(
                ui,
                "Archive-referenced payloads (overlaps the breakdown above)",
                usage.stored.archive_referenced_bytes,
            );
            ui.small("Shared text is counted once. Indexes, page overhead and WAL are included in total files, not in payload rows. Measurements refresh every five seconds; original documents and the application binary are excluded.");
        }
    }
}

fn usage_row(ui: &mut egui::Ui, label: &str, bytes: u64) {
    ui.label(format!(
        "{label}: {:.2} MiB ({bytes} bytes)",
        bytes as f64 / 1_048_576.0
    ));
}
