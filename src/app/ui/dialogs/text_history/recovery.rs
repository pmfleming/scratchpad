use crate::app::{
    app_state::ScratchpadApp,
    ui::recovery::{query, timestamp},
};
use eframe::egui;

pub(super) fn render(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let Some(tab) = app
        .tab_manager
        .tabs
        .as_slice()
        .get(app.tab_manager.active_tab_index)
    else {
        return;
    };
    let buffer = tab.active_buffer();
    let id = buffer.id;
    let name = buffer.name.clone();
    let temp_id = buffer.temp_id.clone();
    ui.label(format!("{name} — snapshot-only recovery"));
    let store = app.state.persistence.session_store.clone();
    let root = store.root().to_owned();
    let selected_key = egui::Id::unique(("history.recovery.selected", &temp_id));
    let versions = query(ui, ("versions", root.clone(), temp_id.clone()), move || {
        store.buffer_versions(&temp_id)
    });
    match versions {
        None => {
            ui.label("Loading recovery points…");
        }
        Some(Err(error)) => {
            ui.label(format!("Could not read recovery points: {error}"));
        }
        Some(Ok(versions)) => {
            if versions.is_empty() {
                ui.label("No recovery points yet. Recent edits are available in edit history.");
            }
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for point in versions.iter() {
                        let label = format!(
                            "{} · {} bytes · {}",
                            timestamp(point.created_at_unix_seconds),
                            point.bytes,
                            if point.undo_boundary {
                                "Before undo compaction"
                            } else {
                                "Automatic snapshot"
                            }
                        );
                        if ui.button(label).clicked() {
                            ui.ctx()
                                .data_mut(|data| data.insert_temp(selected_key, point.id));
                        }
                    }
                });
        }
    }
    if let Some(version) = ui.ctx().data(|data| data.get_temp::<i64>(selected_key)) {
        let store = app.state.persistence.session_store.clone();
        match query(ui, ("version-text", root, version), move || {
            store.restore_buffer_version(version)
        }) {
            None => {
                ui.label("Loading preview…");
            }
            Some(Err(error)) => {
                ui.label(format!("Could not read recovery point: {error}"));
            }
            Some(Ok(text)) => {
                super::preview::open(ui.ctx(), id, format!("{name} (recovery point)"), text);
                ui.ctx().data_mut(|data| data.remove::<i64>(selected_key));
            }
        }
    }
}
