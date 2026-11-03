use super::HistoryUi;
use crate::app::{app_state::ScratchpadApp, domain::BufferState, ui::recovery};
use eframe::egui;

pub(super) fn matches(buffer: &BufferState, search: &str) -> bool {
    crate::app::text_history::buffer_label(buffer)
        .to_lowercase()
        .contains(&search.to_lowercase())
}

pub(super) fn show(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    state: &mut HistoryUi,
) -> Option<String> {
    let store = app.state.persistence.session_store.clone();
    let key = ("history.recent_files", store.root().to_path_buf());
    let closed = recovery::query(ui, key, move || store.recent_history_files());
    let mut files: Vec<_> = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|t| t.buffers())
        .filter(|b| matches(b, &state.search))
        .map(|b| {
            (
                b.document().last_edit(),
                crate::app::text_history::buffer_label(b),
                Some(b.id),
                None,
            )
        })
        .collect();
    if let Some(result) = closed {
        match result {
            Ok(closed) => {
                for file in closed.iter().filter(|f| {
                    f.label
                        .to_lowercase()
                        .contains(&state.search.to_lowercase())
                }) {
                    if app
                        .tab_manager
                        .tabs
                        .as_slice()
                        .iter()
                        .flat_map(|t| t.buffers())
                        .any(|b| b.temp_id == file.temp_id)
                    {
                        continue;
                    }
                    files.push((
                        (file.edited_at, file.edit_seq),
                        file.label.clone(),
                        None,
                        Some(file.token.clone()),
                    ));
                }
            }
            Err(error) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Could not read recent files: {error}"),
                );
            }
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let mut reopen = None;
    egui::ScrollArea::vertical()
        .id_salt("history.recent_files")
        .max_height(120.0)
        .show_rows(ui, 24.0, files.len(), |ui, range| {
            for index in range {
                let (edited, label, id, token) = &files[index];
                ui.horizontal(|ui| {
                    let caption = if token.is_some() {
                        format!("{label} · Closed")
                    } else {
                        label.clone()
                    };
                    if ui
                        .selectable_label(id.is_some() && *id == state.selected, caption)
                        .clicked()
                    {
                        if id.is_some() {
                            state.selected = *id;
                            state.all_files = false;
                        } else {
                            reopen.clone_from(token);
                        }
                    }
                    if edited.0 != 0 {
                        ui.weak(recovery::timestamp(edited.0 as i64));
                    }
                });
            }
        });
    reopen
}
