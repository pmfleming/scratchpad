use crate::app::{
    app_state::{ScratchpadApp, workspace_controller},
    services::session_store::recovery::RecoveryPreview,
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
            egui::ScrollArea::vertical()
                .id_salt("recovery-items")
                .max_height(110.0)
                .show(ui, |ui| {
                    for item in items.iter() {
                        ui.horizontal(|ui| {
                            ui.label(&item.name)
                                .on_hover_text(item.path.as_ref().map_or_else(
                                    || "Unsaved document".into(),
                                    |p| p.display().to_string(),
                                ));
                            if ui.button("Review recovery").clicked() {
                                state.selected = Some(item.id.clone());
                                state.compare = false;
                                state.save = None;
                            }
                        });
                    }
                });
            if let Some(id) = state.selected.clone() {
                let store = app.state.persistence.session_store.clone();
                let preview = recovery::query_with_interval(
                    ui,
                    ("recovery-preview", &root, id.clone()),
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
                        render_preview(ui, app, &preview, &mut state, &root);
                    }
                }
            }
            ui.separator();
        }
        Some(Ok(_)) => {
            state.selected = None;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, state));
}

fn render_preview(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    preview: &Arc<RecoveryPreview>,
    state: &mut ViewState,
    root: &PathBuf,
) {
    ui.strong(&preview.item.name);
    if preview.gaps == 0 {
        ui.label("Current text verified. Recovered copies start a new undo history; the original history stays protected.");
    } else {
        ui.colored_label(ui.visuals().warn_fg_color, format!("Partial recovery: {} marked gap(s). Copies include these markers; review them before use.", preview.gaps));
    }
    if preview.truncated {
        ui.label("Recovery size limit reached. This preview and its copies are incomplete; the original data is retained.");
    }
    ui.collapsing("Technical details", |ui| {
        ui.label(&preview.item.reason);
    });
    ui.horizontal_wrapped(|ui| {
        if ui.button("Open editable recovered copy").clicked() {
            workspace_controller::append_created_tab(app, preview.editable_copy());
            app.state.dialogs.status_history.close();
        }
        let saving = state.save.as_ref().is_some_and(|s| s.lock().unwrap_or_else(|e| e.into_inner()).is_none());
        if ui.add_enabled(!saving, egui::Button::new("Save recovered copy…")).clicked() {
            let result: SaveResult = Arc::default();
            state.save = Some(result.clone());
            let preview = preview.clone();
            let ctx = ui.ctx().clone();
            let output = result.clone();
            if let Err(error) = std::thread::Builder::new().name("recovery-export".into()).spawn(move || {
                let outcome = (|| {
                    let name = format!("{}.recovered.txt", preview.item.name);
                    let Some(path) = crate::app::platform_file::pick_save_as_path(&name) else { return Ok(None); };
                    preview.save_copy(&path).map_err(|e| format!("Could not save copy: {e}. Choose a new filename; existing files are never overwritten."))?;
                    Ok(Some(path))
                })();
                *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
                ctx.request_repaint();
            }) {
                *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(error.to_string()));
            }
        }
        if ui.add_enabled(preview.item.path.is_some(), egui::Button::new("Compare with disk")).clicked() {
            state.compare = true;
            recovery::refresh_query::<String>(ui, ("recovery-disk", root, &preview.item.id));
        }
    });
    if let Some(save) = &state.save {
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
    if state.compare {
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
    } else {
        text_preview(ui, "Recovered text", &preview.text);
    }
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
    use super::{ViewState, render_preview};
    use crate::app::{
        app_state::test_support::{test_app, ui_harness},
        services::session_store::recovery::{RecoveryItem, RecoveryPreview},
    };
    use eframe::egui;
    use egui_kittest::kittest::Queryable;
    use std::sync::Arc;

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
}
