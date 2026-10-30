use super::super::menu_ui::{OPEN_FILE_SUBMENU_WIDTH as LIST_WIDTH, ROW_HEIGHT, with_row_visuals};
use crate::app::{
    app_state::{ScratchpadApp, workspace::lifecycle},
    services::{session_store::ClosedTabInfo, settings_store::RecentlyClosedFile},
    ui::{recovery, widget_ids},
};
use eframe::egui;
use egui_phosphor::regular::{DOT, FILE, FILE_X, WARNING_CIRCLE};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiskStatus {
    OnDisk,
    Missing,
    Unavailable,
}

impl DiskStatus {
    fn read(path: &Path) -> Self {
        Self::from_metadata(std::fs::metadata(path))
    }

    fn from_metadata(result: io::Result<std::fs::Metadata>) -> Self {
        match result {
            Ok(metadata) if metadata.is_file() => Self::OnDisk,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self::Missing,
            _ => Self::Unavailable,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum ReopenTarget {
    Archive(String),
    File(PathBuf),
}

struct ReopenEntry {
    target: ReopenTarget,
    name: String,
    closed_at: Option<i64>,
    unsaved: bool,
    paths: Vec<(PathBuf, DiskStatus)>,
}

impl ReopenEntry {
    fn can_reopen(&self) -> bool {
        matches!(self.target, ReopenTarget::Archive(_))
            || self
                .paths
                .iter()
                .all(|(_, status)| *status == DiskStatus::OnDisk)
    }

    fn disk_icon(&self) -> Option<&'static str> {
        if self.paths.is_empty() {
            None
        } else if self
            .paths
            .iter()
            .any(|(_, status)| *status == DiskStatus::Unavailable)
        {
            Some(WARNING_CIRCLE)
        } else if self
            .paths
            .iter()
            .any(|(_, status)| *status == DiskStatus::Missing)
        {
            Some(FILE_X)
        } else {
            Some(FILE)
        }
    }

    fn tooltip(&self) -> String {
        let mut lines = vec![self.closed_at.map_or_else(
            || "Closed time unknown".into(),
            |closed_at| format!("Closed {}", recovery::timestamp(closed_at)),
        )];
        if self.paths.is_empty() {
            lines.push("No file location".into());
        } else {
            lines.extend(
                self.paths
                    .iter()
                    .map(|(path, _)| path.display().to_string()),
            );
        }
        lines.join("\n")
    }
}

/// Runs in the recovery-query worker, including path matching and disk checks.
fn merged_entries(
    archives: Vec<ClosedTabInfo>,
    files: Vec<RecentlyClosedFile>,
) -> Vec<ReopenEntry> {
    let mut entries: Vec<_> = archives
        .iter()
        .map(|archive| ReopenEntry {
            target: ReopenTarget::Archive(archive.token.clone()),
            name: archive.name.clone(),
            closed_at: Some(archive.closed_at_unix_seconds),
            unsaved: archive.has_unsaved_changes,
            paths: archive
                .paths
                .iter()
                .map(|path| (path.clone(), DiskStatus::read(path)))
                .collect(),
        })
        .collect();
    for file in files {
        if entries.iter().any(|entry| {
            entry
                .paths
                .iter()
                .any(|(saved, _)| crate::app::paths_match(saved, &file.path))
        }) {
            continue;
        }
        entries.push(ReopenEntry {
            name: file.path.file_name().map_or_else(
                || file.path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            closed_at: file.closed_at_unix_seconds,
            unsaved: false,
            paths: vec![(file.path.clone(), DiskStatus::read(&file.path))],
            target: ReopenTarget::File(file.path),
        });
    }
    // Stable ordering keeps legacy path-only records in their original order,
    // after timestamped entries. Distinct archived versions remain separate.
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.closed_at));
    entries
}

struct ReopenList {
    entries: Vec<ReopenEntry>,
    archive_error: Option<String>,
}

pub(super) fn render_recently_closed_files(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
) {
    let store = app.state.persistence.session_store.clone();
    let root = store.root().to_owned();
    let files: Vec<_> = app
        .state
        .recently_closed_files
        .iter()
        .filter(|_| app.state.app_settings.recent_files_enabled())
        .cloned()
        .collect();
    let key = ("closed.unified", root, files.clone());
    let list = match recovery::query(ui, key, move || {
        let (archives, archive_error) = match store.closed_tabs() {
            Ok(archives) => (archives, None),
            Err(error) => (Vec::new(), Some(error.to_string())),
        };
        Ok(ReopenList {
            entries: merged_entries(archives, files),
            archive_error,
        })
    }) {
        Some(Ok(list)) => list,
        Some(Err(error)) => {
            ui.label(format!("Could not read recently closed items: {error}"));
            return;
        }
        None => {
            ui.label("Loading recently closed items…");
            return;
        }
    };
    if let Some(error) = &list.archive_error {
        ui.label(format!("Could not read retained tabs: {error}"));
    }
    if list.entries.is_empty() {
        if list.archive_error.is_none() {
            ui.label("No recently closed files");
        }
        return;
    }
    let mut names = HashMap::<&str, usize>::new();
    for entry in &list.entries {
        *names.entry(&entry.name).or_default() += 1;
    }
    let restore_ready =
        !app.startup_restore_pending() && !app.state.persistence.session_restore_failed;
    ui.set_min_width(LIST_WIDTH);
    ui.set_max_width(LIST_WIDTH);
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .show(ui, |ui| {
            for entry in &list.entries {
                let label = if names[entry.name.as_str()] > 1 {
                    entry.closed_at.map_or_else(
                        || entry.name.clone(),
                        |at| format!("{} · {}", entry.name, recovery::timestamp(at)),
                    )
                } else {
                    entry.name.clone()
                };
                let enabled = entry.can_reopen()
                    && (restore_ready || matches!(entry.target, ReopenTarget::File(_)));
                if render_entry(ui, entry, &label, enabled) {
                    match &entry.target {
                        ReopenTarget::Archive(token) => lifecycle::reopen_archived_tab(app, token),
                        ReopenTarget::File(path) => {
                            super::activate_slot(app, slot_index);
                            super::open_recent_file(app, path.clone());
                        }
                    }
                    ui.close();
                }
            }
        });
}

fn render_entry(ui: &mut egui::Ui, entry: &ReopenEntry, label: &str, enabled: bool) -> bool {
    with_row_visuals(ui, |ui| {
        let response = widget_ids::surface_response(
            ui,
            ("tab_context.reopen", &entry.target),
            widget_ids::WidgetRole::ActionButton,
            |ui| {
                ui.add_enabled(
                    enabled,
                    egui::Button::new("")
                        .min_size(egui::vec2(LIST_WIDTH, ROW_HEIGHT))
                        .stroke(egui::Stroke::NONE),
                )
            },
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
        let color =
            crate::app::theme::text_primary(ui).gamma_multiply(if enabled { 1.0 } else { 0.45 });
        let center = response.rect.left_center();
        if let Some(icon) = entry.disk_icon() {
            ui.painter().text(
                center + egui::vec2(16.0, 0.0),
                egui::Align2::CENTER_CENTER,
                icon,
                egui_phosphor::font_id(16.0),
                color,
            );
        }
        if entry.unsaved {
            ui.painter().text(
                center + egui::vec2(36.0, 0.0),
                egui::Align2::CENTER_CENTER,
                DOT,
                egui_phosphor::font_id(16.0),
                color,
            );
        }
        let text_rect = egui::Rect::from_min_max(
            response.rect.left_top() + egui::vec2(52.0, 0.0),
            response.rect.right_bottom() - egui::vec2(8.0, 0.0),
        );
        ui.painter().with_clip_rect(text_rect).text(
            text_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::TextStyle::Button.resolve(ui.style()),
            color,
        );
        let clicked = response.clicked();
        let tooltip = entry.tooltip();
        response
            .on_hover_text(&tooltip)
            .on_disabled_hover_text(tooltip);
        clicked
    })
}
