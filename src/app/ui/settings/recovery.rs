use super::{ScratchpadApp, category_heading, egui, expandable_card};
use crate::app::services::session_store::StorageUsage;
use std::{sync::{Arc, Mutex}, time::{Duration, Instant}};

#[derive(Clone, Default)]
struct UsageCache {
    result: Option<Result<StorageUsage, String>>,
    checked: Option<Instant>,
    pending: bool,
}

pub(super) fn render_recovery_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_heading(ui, "History & Recovery");
    super::sections::text_history_budget_card(ui, app);
    ui.add_space(super::SettingsUi::LAYOUT.card_gap);
    expandable_card(ui, "recovery.policy", egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
        "Automatic recovery", "Retention and reopening policy.", false, |ui| {
        let mut policy = app.state.app_settings.history.recovery.clone();
        super::sections::byte_budget_row(ui, "Recovery points", "Global stored snapshot budget.", &mut policy.snapshot_byte_budget, 1, 16384);
        super::sections::byte_budget_row(ui, "Closed archives", "Stored archive budget; protected archives may exceed it.", &mut policy.archive_byte_budget, 1, 65536);
        ui.horizontal(|ui| {
            ui.label("Archive retention (days; 0 = no age expiry)");
            ui.add(egui::DragValue::new(&mut policy.archive_retention_days).range(0..=36500));
        });
        ui.checkbox(&mut policy.reopen_archives, "Restore archived tabs when reopening files");
        ui.label("Recent edits are undoable. Older states become snapshot-only recovery points. When budgets are full, older unprotected recovery data expires automatically.");
        ui.label("External changes: open current disk text; preserve conflicting unsaved text in a separate recovered tab. Missing files recover as untitled tabs. Original files are never overwritten automatically.");
        ui.label("Pinned archives never expire automatically. They may exceed the archive budget; unpin them in Recently Closed to allow cleanup.");
        ui.label("Budgets count stored payloads, not database allocation. Shared chunks, indexes, reusable pages and the write-ahead log affect actual disk usage.");
        if policy != app.state.app_settings.history.recovery {
            crate::app::app_state::settings_controller::set_recovery_settings(app, policy);
        }
    });
    ui.add_space(super::SettingsUi::LAYOUT.card_gap);
    expandable_card(ui, "recovery.storage", egui_phosphor::regular::FLOPPY_DISK,
        "Disk usage", "Actual data-directory usage and stored recovery data.", false, |ui| {
        render_usage(ui, app);
    });
}

fn render_usage(ui: &mut egui::Ui, app: &ScratchpadApp) {
    let store = app.state.persistence.session_store.clone();
    let key = egui::Id::unique(("recovery.storage.cache", store.root()));
    let cache = ui.ctx().data_mut(|data| data.get_temp_mut_or_default::<Arc<Mutex<UsageCache>>>(key).clone());
    let mut state = cache.lock().unwrap_or_else(|e| e.into_inner());
    if !state.pending && state.checked.is_none_or(|at| at.elapsed() >= Duration::from_secs(5)) {
        state.pending = true;
        let cache = cache.clone();
        let ctx = ui.ctx().clone();
        let spawned = std::thread::Builder::new().name("storage-usage".into()).spawn(move || {
            let result = store.storage_usage().map_err(|error| error.to_string());
            let mut state = cache.lock().unwrap_or_else(|e| e.into_inner());
            state.result = Some(result);
            state.checked = Some(Instant::now());
            state.pending = false;
            ctx.request_repaint();
        });
        if let Err(error) = spawned {
            state.pending = false;
            state.checked = Some(Instant::now());
            state.result = Some(Err(error.to_string()));
        }
    }
    ui.ctx().request_repaint_after(Duration::from_secs(5));
    ui.label(format!("Data directory: {}", app.state.persistence.session_store.root().display()));
    match &state.result {
        None => { ui.label("Measuring disk usage…"); }
        Some(Err(error)) => { ui.label(format!("Could not measure disk usage: {error}")); }
        Some(Ok(usage)) => {
            for (label, bytes) in [
                ("Total files", usage.total_file_bytes),
                ("Session database (all recovery data)", usage.database),
                ("Write-ahead log / shared memory", usage.journal),
                ("Other data-directory files", usage.other),
            ] { usage_row(ui, label, bytes); }
            if let Some(bytes) = usage.allocated_bytes { usage_row(ui, "Total allocated on disk", bytes); }
            usage_row(ui, "Reusable space inside database", usage.stored.reusable_pages);
            ui.separator();
            ui.label("Stored payload breakdown (not exclusive disk allocation)");
            for (label, bytes) in [
                ("Session metadata", usage.stored.session_metadata),
                ("Undo data (open and archived)", usage.stored.undo),
                ("Closed archive metadata", usage.stored.archive_metadata),
                ("Recovery point metadata (estimate)", usage.stored.snapshot_metadata),
                ("Current / archived text only", usage.stored.live_text),
                ("Historical snapshot text only", usage.stored.recovery_text),
                ("Text shared by both", usage.stored.shared_text),
                ("Unreferenced text awaiting cleanup", usage.stored.unreferenced_text),
            ] { usage_row(ui, label, bytes); }
            ui.small("Shared text is counted once. Indexes, page overhead and WAL are included in total files, not in payload rows. Measurements refresh every five seconds; original documents and the application binary are excluded.");
        }
    }
}

fn usage_row(ui: &mut egui::Ui, label: &str, bytes: u64) {
    ui.label(format!("{label}: {:.2} MiB ({bytes} bytes)", bytes as f64 / 1_048_576.0));
}
