use crate::app::{
    app_state::{ScratchpadApp, workspace_controller},
    services::session_store::recovery::{RecoveryItem, RecoveryPreview},
    ui::recovery,
};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

type SaveResult = Arc<Mutex<Option<Result<Option<PathBuf>, String>>>>;
#[derive(Clone, Default)]
struct ViewState {
    selected: Option<String>,
    compare: bool,
    save: Option<SaveResult>,
}

pub(crate) fn render(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let store = app.state.persistence.session_store.clone();
    if store.storage_unavailable() {
        return;
    }
    let root = store.root().to_owned();
    let key = egui::Id::unique(("recovery-panel", &root));
    let mut state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<ViewState>(key).unwrap_or_default());
    let items = recovery::query(ui, ("recovery-items", &root), move || {
        store.recovery_items()
    });
    match items {
        None => {
            ui.label("Checking recovery…");
        }
        Some(Err(error)) => {
            ui.label(format!("Recovery details unavailable: {error}"));
        }
        Some(Ok(items)) if !items.is_empty() => {
            ui.heading("Document recovery");
            ui.label("Protected originals are retained. Other documents continue saving normally.");
            item_list(ui, &items, &mut state);
            if let Some(id) = state.selected.clone() {
                selected_preview(ui, app, id, &mut state, &root);
            }
            ui.separator();
        }
        Some(Ok(_)) => {
            state.selected = None;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, state));
}

/// Each protected document with a button that opens its review.
fn item_list(ui: &mut egui::Ui, items: &[RecoveryItem], state: &mut ViewState) {
    egui::ScrollArea::vertical()
        .id_salt("recovery-items")
        .max_height(110.0)
        .show(ui, |ui| {
            for item in items {
                ui.horizontal(|ui| {
                    ui.label(&item.name).on_hover_text(
                        item.path
                            .as_ref()
                            .map_or_else(|| "Unsaved document".into(), |p| p.display().to_string()),
                    );
                    if ui.button("Review recovery").clicked() {
                        state.selected = Some(item.id.clone());
                        state.compare = false;
                        state.save = None;
                    }
                });
            }
        });
}

fn selected_preview(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    id: String,
    state: &mut ViewState,
    root: &PathBuf,
) {
    let store = app.state.persistence.session_store.clone();
    let preview = recovery::query_with_interval(
        ui,
        ("recovery-preview", root, id.clone()),
        Duration::from_hours(1),
        move || store.recovery_preview(id),
    );
    match preview {
        None => {
            ui.label("Verifying recoverable text…");
        }
        Some(Err(error)) => {
            ui.label(format!(
                "Preview unavailable: {error}. Original data remains protected."
            ));
        }
        Some(Ok(preview)) => {
            render_preview(ui, app, &preview, state, root);
        }
    }
}

fn render_preview(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    preview: &Arc<RecoveryPreview>,
    state: &mut ViewState,
    root: &PathBuf,
) {
    preview_notes(ui, preview);
    ui.horizontal_wrapped(|ui| {
        if ui.button("Open editable recovered copy").clicked() {
            workspace_controller::append_created_tab(app, preview.editable_copy());
            app.state.dialogs.status_history.close();
        }
        let saving = state
            .save
            .as_ref()
            .is_some_and(|s| s.lock().unwrap_or_else(|e| e.into_inner()).is_none());
        if ui
            .add_enabled(!saving, egui::Button::new("Save recovered copy…"))
            .clicked()
        {
            state.save = Some(start_export(ui.ctx(), preview));
        }
        if ui
            .add_enabled(
                preview.item.path.is_some(),
                egui::Button::new("Compare with disk"),
            )
            .clicked()
        {
            state.compare = true;
            recovery::refresh_query::<String>(ui, ("recovery-disk", root, &preview.item.id));
        }
    });
    if let Some(save) = &state.save {
        export_status(ui, save);
    }
    if state.compare {
        comparison(ui, preview, root);
    } else {
        text_preview(ui, "Recovered text", &preview.text);
    }
}

/// The document's name, how complete the recovered text is, and why it was protected.
fn preview_notes(ui: &mut egui::Ui, preview: &RecoveryPreview) {
    ui.strong(&preview.item.name);
    if preview.gaps == 0 {
        ui.label("Current text verified. Recovered copies start a new undo history; the original history stays protected.");
    } else {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!(
                "Partial recovery: {} marked gap(s). Copies include these markers; review them before use.",
                preview.gaps
            ),
        );
    }
    if preview.truncated {
        ui.label("Recovery size limit reached. This preview and its copies are incomplete; the original data is retained.");
    }
    ui.collapsing("Technical details", |ui| {
        ui.label(&preview.item.reason);
    });
}

/// Asks for a new file name off the UI thread and writes the recovered copy there.
fn start_export(ctx: &egui::Context, preview: &Arc<RecoveryPreview>) -> SaveResult {
    let result: SaveResult = Arc::default();
    let output = result.clone();
    let preview = preview.clone();
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new()
        .name("recovery-export".into())
        .spawn(move || {
            let outcome = export_copy(&preview);
            *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
            ctx.request_repaint();
        });
    if let Err(error) = spawned {
        *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(error.to_string()));
    }
    result
}

fn export_copy(preview: &RecoveryPreview) -> Result<Option<PathBuf>, String> {
    let name = format!("{}.recovered.txt", preview.item.name);
    let Some(path) = crate::app::platform_file::pick_save_as_path(&name) else {
        return Ok(None);
    };
    preview.save_copy(&path).map_err(|e| {
        format!(
            "Could not save copy: {e}. Choose a new filename; existing files are never overwritten."
        )
    })?;
    Ok(Some(path))
}

fn export_status(ui: &mut egui::Ui, save: &SaveResult) {
    match &*save.lock().unwrap_or_else(|e| e.into_inner()) {
        None => {
            ui.label("Waiting for recovery export…");
        }
        Some(Ok(Some(path))) => {
            ui.label(format!("Recovered copy saved to {}", path.display()));
        }
        Some(Ok(None)) => {
            ui.label("Export cancelled.");
        }
        Some(Err(error)) => {
            ui.label(error);
        }
    }
}

/// The recovered text beside the file currently on disk.
fn comparison(ui: &mut egui::Ui, preview: &Arc<RecoveryPreview>, root: &PathBuf) {
    let copy = preview.clone();
    let disk = recovery::query_with_interval(
        ui,
        ("recovery-disk", root, &preview.item.id),
        Duration::from_hours(1),
        move || copy.disk_text(),
    );
    ui.columns(2, |columns| {
        text_preview(&mut columns[0], "Recovered text", &preview.text);
        match disk {
            None => {
                columns[1].label("Reading disk file…");
            }
            Some(Err(error)) => {
                columns[1].label(format!("Disk comparison unavailable: {error}"));
            }
            Some(Ok(text)) => {
                text_preview(&mut columns[1], "Current disk file", &text);
            }
        }
    });
}

pub(super) fn text_preview(ui: &mut egui::Ui, title: &str, text: &str) {
    ui.label(title);
    // Keep paint work bounded; exports and editable copies use the full recovery result.
    let end = text
        .char_indices()
        .nth(16_000)
        .map_or(text.len(), |(i, _)| i);
    let mut visible = &text[..end];
    egui::ScrollArea::both()
        .id_salt(title)
        .max_height(180.0)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut visible)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
        });
    if end < text.len() {
        ui.label("Showing the first 16,000 characters. Open or save a recovered copy to inspect the rest.");
    }
}

#[cfg(test)]
mod tests {
    use super::{SaveResult, ViewState, render_preview};
    use crate::app::{
        app_state::test_support::{test_app, ui_harness, write_file},
        services::session_store::recovery::{RecoveryItem, RecoveryPreview},
    };
    use eframe::egui;
    use egui_kittest::kittest::Queryable;
    use std::sync::{Arc, Mutex};

    #[test]
    fn partial_recovery_ui_opens_a_separate_dirty_copy_with_gap_markers() {
        let root = tempfile::tempdir().unwrap();
        let app = test_app(root.path());
        let preview = Arc::new(RecoveryPreview {
            item: RecoveryItem {
                id: "protected-id".into(),
                name: "draft.txt".into(),
                path: None,
                reason: "missing text".into(),
            },
            text: "good text\n[RECOVERY GAP: missing text]\n".into(),
            gaps: 1,
            truncated: false,
        });
        let expected = preview.text.clone();
        let mut state = ViewState::default();
        let path = root.path().to_owned();
        let mut harness = ui_harness(egui::vec2(1100., 700.), app, move |ui, app| {
            render_preview(ui, app, &preview, &mut state, &path);
        });
        harness.get_by_label("Partial recovery: 1 marked gap(s). Copies include these markers; review them before use.");
        harness.get_by_label("Open editable recovered copy").click();
        harness.run_steps(2);
        let copy = harness
            .state()
            .tab_manager
            .tabs
            .iter()
            .find(|tab| tab.active_buffer().name == "draft.txt (recovered)")
            .unwrap();
        assert_eq!(copy.active_buffer().text(), expected);
        assert!(copy.active_buffer().path.is_none());
        assert!(copy.active_buffer().is_dirty);
        assert_ne!(copy.active_buffer().temp_id, "protected-id");
    }

    #[test]
    fn verified_preview_reports_export_outcomes_and_compares_with_disk() {
        let root = tempfile::tempdir().unwrap();
        let disk = write_file(root.path(), "draft.txt", "disk text");
        let preview = Arc::new(RecoveryPreview {
            item: RecoveryItem {
                id: "verified-id".into(),
                name: "draft.txt".into(),
                path: Some(disk.clone()),
                reason: "session could not be opened".into(),
            },
            text: "recovered text".into(),
            gaps: 0,
            truncated: true,
        });
        let outcomes = [
            (None, "Waiting for recovery export…".into()),
            (Some(Ok(None)), "Export cancelled.".into()),
            (Some(Err("disk full".into())), "disk full".into()),
            (
                Some(Ok(Some(disk.clone()))),
                format!("Recovered copy saved to {}", disk.display()),
            ),
        ];
        for (outcome, label) in outcomes {
            let save: SaveResult = Arc::new(Mutex::new(outcome));
            let mut state = ViewState {
                selected: Some("verified-id".into()),
                compare: true,
                save: Some(save),
            };
            let preview = preview.clone();
            let path = root.path().to_owned();
            let mut harness = ui_harness(
                egui::vec2(1100., 700.),
                test_app(root.path()),
                move |ui, app| {
                    render_preview(ui, app, &preview, &mut state, &path);
                },
            );
            harness.get_by_label_contains("Current text verified");
            harness.get_by_label_contains("Recovery size limit reached");
            harness.get_by_label(&label);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while harness.query_by_label("Current disk file").is_none() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "disk text never loaded"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
                harness.run_steps(1);
            }
            harness.get_by_label("Recovered text");
        }
    }
}
