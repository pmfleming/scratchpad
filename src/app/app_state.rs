use crate::app::chrome::handle_window_resize;
use crate::app::commands::AppCommand;
use crate::app::domain::{
    BufferState, EditorViewState, SplitAxis, SplitPath, ViewId, WorkspaceTab,
};
use crate::app::services::file_service::FileService;
use crate::app::services::session_manager;
use crate::app::services::session_store::SessionStore;
use crate::app::shortcuts;
use crate::app::ui::{dialogs, editor_area, status_bar, tab_strip};
use crate::app::{paths_match, theme};
use eframe::egui;
use std::path::Path;
use std::time::{Duration, Instant};

pub(crate) const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy)]
pub(crate) enum PendingAction {
    CloseTab(usize),
}

pub struct ScratchpadApp {
    pub(crate) tabs: Vec<WorkspaceTab>,
    pub(crate) active_tab_index: usize,
    pub(crate) pending_action: Option<PendingAction>,
    pub(crate) font_size: f32,
    pub(crate) word_wrap: bool,
    pub(crate) status_message: Option<String>,
    pub(crate) session_store: SessionStore,
    pub(crate) session_dirty: bool,
    pub(crate) last_session_persist: Instant,
    pub(crate) close_in_progress: bool,
    pub(crate) pending_scroll_to_active: bool,
    pub(crate) overflow_popup_open: bool,
}

enum OpenPathOutcome {
    Opened { artifact_warning: Option<String> },
    AlreadyOpen,
    Failed,
}

impl Default for ScratchpadApp {
    fn default() -> Self {
        Self::with_session_store(SessionStore::default())
    }
}

impl eframe::App for ScratchpadApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|input| input.viewport().close_requested()) && !self.close_in_progress {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_exit(&ctx);
            return;
        }

        handle_window_resize(&ctx);
        session_manager::maybe_persist_session(self, &ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        tab_strip::show_header(ui, self);
        status_bar::show_status_bar(ui, self);
        editor_area::show_editor(ui, self);
        dialogs::show_pending_action_modal(&ctx, self);
        shortcuts::handle_shortcuts(self, &ctx);
        let _ = frame;
    }
}

impl Drop for ScratchpadApp {
    fn drop(&mut self) {
        let _ = self.persist_session_now();
    }
}

impl ScratchpadApp {
    pub fn with_session_store(session_store: SessionStore) -> Self {
        let mut app = Self {
            tabs: vec![WorkspaceTab::untitled()],
            active_tab_index: 0,
            pending_action: None,
            font_size: 14.0,
            word_wrap: true,
            status_message: None,
            session_store,
            session_dirty: false,
            last_session_persist: Instant::now(),
            close_in_progress: false,
            pending_scroll_to_active: true,
            overflow_popup_open: false,
        };

        session_manager::restore_session_state(&mut app);

        app
    }

    pub(crate) fn active_tab(&self) -> Option<&WorkspaceTab> {
        self.tabs.get(self.active_tab_index)
    }

    pub(crate) fn active_view_mut(&mut self) -> Option<&mut EditorViewState> {
        self.tabs
            .get_mut(self.active_tab_index)
            .and_then(WorkspaceTab::active_view_mut)
    }

    pub(crate) fn mark_session_dirty(&mut self) {
        self.session_dirty = true;
    }

    pub(crate) fn persist_session_now(&mut self) -> std::io::Result<()> {
        session_manager::persist_session_now(self)
    }

    pub(crate) fn estimated_tab_strip_width(&self, spacing: f32) -> f32 {
        if self.tabs.is_empty() {
            return 0.0;
        }

        (self.tabs.len() as f32 * theme::TAB_BUTTON_WIDTH)
            + ((self.tabs.len().saturating_sub(1)) as f32 * spacing)
    }

    pub(crate) fn request_exit(&mut self, ctx: &egui::Context) {
        if self.close_in_progress {
            return;
        }

        match self.persist_session_now() {
            Ok(()) => {
                self.close_in_progress = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(error) => {
                self.status_message = Some(format!("Session save failed: {error}"));
            }
        }
    }

    pub fn new_tab(&mut self) {
        self.create_untitled_tab();
        let _ = self.persist_session_now();
    }

    pub fn open_file(&mut self) {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            self.open_selected_paths(paths);
        }
    }

    pub fn save_file(&mut self) {
        let _ = self.save_file_at(self.active_tab_index);
    }

    pub fn save_file_at(&mut self, index: usize) -> bool {
        if self.tabs.is_empty() {
            return false;
        }

        if self.tabs[index].buffer.path.is_some() {
            self.save_existing_path(index)
        } else {
            self.save_file_as_at(index);
            !self.tabs[index].buffer.is_dirty
        }
    }

    pub fn save_file_as(&mut self) {
        let _ = self.save_file_as_at(self.active_tab_index);
    }

    pub fn save_file_as_at(&mut self, index: usize) -> bool {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&self.tabs[index].buffer.name)
            .save_file()
        {
            self.save_buffer_to_path(index, path, true)
        } else {
            self.status_message = Some("Save cancelled.".to_owned());
            false
        }
    }

    pub(crate) fn perform_close_tab(&mut self, index: usize) {
        self.close_tab_internal(index);
        let _ = self.persist_session_now();
    }

    pub fn perform_close_tab_no_persist(&mut self, index: usize) {
        self.close_tab_internal(index);
    }

    fn close_tab_internal(&mut self, index: usize) {
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.tabs.push(WorkspaceTab::untitled());
            self.active_tab_index = 0;
            return;
        }

        if self.active_tab_index > index {
            self.active_tab_index -= 1;
        }
        self.active_tab_index = self.active_tab_index.min(self.tabs.len() - 1);
        self.pending_scroll_to_active = true;
    }

    pub(crate) fn window_title(&self) -> String {
        if self.tabs.is_empty() {
            return "Scratchpad".to_owned();
        }

        let tab = &self.tabs[self.active_tab_index.min(self.tabs.len() - 1)];
        let marker = if tab.buffer.is_dirty { "*" } else { "" };
        format!("{}{} - Scratchpad", marker, tab.buffer.name)
    }

    pub(crate) fn split_active_view_with_placement(
        &mut self,
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    ) {
        self.handle_command(AppCommand::SplitActiveView {
            axis,
            new_view_first,
            ratio,
        });
    }

    pub(crate) fn close_view(&mut self, view_id: ViewId) {
        self.handle_command(AppCommand::CloseView { view_id });
    }

    pub(crate) fn activate_view(&mut self, view_id: ViewId) {
        self.handle_command(AppCommand::ActivateView { view_id });
    }

    pub(crate) fn resize_split(&mut self, path: SplitPath, ratio: f32) {
        self.handle_command(AppCommand::ResizeSplit { path, ratio });
    }

    fn append_tab(&mut self, tab: WorkspaceTab) {
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
        self.pending_scroll_to_active = true;
    }

    pub fn create_untitled_tab(&mut self) {
        self.append_tab(WorkspaceTab::untitled());
    }

    pub fn tabs(&self) -> &[WorkspaceTab] {
        &self.tabs
    }

    pub fn tabs_mut(&mut self) -> &mut [WorkspaceTab] {
        &mut self.tabs
    }

    pub fn active_tab_index(&self) -> usize {
        self.active_tab_index
    }

    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    pub fn word_wrap(&self) -> bool {
        self.word_wrap
    }

    pub fn session_store(&self) -> &SessionStore {
        &self.session_store
    }

    fn find_tab_by_path(&self, candidate: &Path) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            tab.buffer
                .path
                .as_deref()
                .is_some_and(|path| paths_match(path, candidate))
        })
    }

    fn open_selected_paths(&mut self, paths: Vec<std::path::PathBuf>) {
        let mut opened_count = 0usize;
        let mut duplicate_count = 0usize;
        let mut failure_count = 0usize;
        let mut artifact_count = 0usize;
        let mut last_artifact_warning = None;

        for path in paths {
            match self.open_path(path) {
                OpenPathOutcome::Opened { artifact_warning } => {
                    opened_count += 1;
                    if let Some(warning) = artifact_warning {
                        artifact_count += 1;
                        last_artifact_warning = Some(warning);
                    }
                }
                OpenPathOutcome::AlreadyOpen => {
                    duplicate_count += 1;
                }
                OpenPathOutcome::Failed => {
                    failure_count += 1;
                }
            }
        }

        self.status_message = summarize_open_results(
            opened_count,
            duplicate_count,
            failure_count,
            artifact_count,
            last_artifact_warning,
        );
    }

    fn open_path(&mut self, path: std::path::PathBuf) -> OpenPathOutcome {
        if self.activate_existing_path(&path).is_some() {
            return OpenPathOutcome::AlreadyOpen;
        }

        match FileService::read_file(&path) {
            Ok(file_content) => OpenPathOutcome::Opened {
                artifact_warning: self.open_loaded_file(path, file_content),
            },
            Err(_) => OpenPathOutcome::Failed,
        }
    }

    fn activate_existing_path(&mut self, path: &Path) -> Option<String> {
        if let Some(index) = self.find_tab_by_path(path) {
            self.handle_command(AppCommand::ActivateTab { index });
            Some(
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string()),
            )
        } else {
            None
        }
    }

    fn open_loaded_file(
        &mut self,
        path: std::path::PathBuf,
        file_content: crate::app::services::file_service::FileContent,
    ) -> Option<String> {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut buffer = BufferState::with_encoding(
            name,
            file_content.content,
            Some(path),
            file_content.encoding,
            file_content.has_bom,
        );
        buffer.artifact_summary = file_content.artifact_summary;
        let artifact_warning = buffer
            .artifact_summary
            .status_text()
            .map(|message| format!("Opened with formatting artifacts: {message}"));
        self.append_tab(WorkspaceTab::new(buffer));
        let _ = self.persist_session_now();
        artifact_warning
    }

    fn save_existing_path(&mut self, index: usize) -> bool {
        let path = self.tabs[index].buffer.path.clone().unwrap();
        self.save_buffer_to_path(index, path, false)
    }

    fn save_buffer_to_path(
        &mut self,
        index: usize,
        path: std::path::PathBuf,
        update_buffer_path: bool,
    ) -> bool {
        let save_result = {
            let buffer = &self.tabs[index].buffer;
            FileService::write_file_with_bom(
                &path,
                &buffer.content,
                &buffer.encoding,
                buffer.has_bom,
            )
        };

        match save_result {
            Ok(()) => {
                self.finalize_save(index, path, update_buffer_path);
                true
            }
            Err(error) => {
                self.status_message = Some(format!("Save failed: {error}"));
                false
            }
        }
    }

    fn finalize_save(&mut self, index: usize, path: std::path::PathBuf, update_buffer_path: bool) {
        let buffer = &mut self.tabs[index].buffer;
        if update_buffer_path {
            buffer.path = Some(path.clone());
            buffer.name = path.file_name().unwrap().to_string_lossy().into_owned();
        }
        buffer.is_dirty = false;
        self.status_message = None;
        self.mark_session_dirty();
        let _ = self.persist_session_now();
    }
}

fn summarize_open_results(
    opened_count: usize,
    duplicate_count: usize,
    failure_count: usize,
    artifact_count: usize,
    last_artifact_warning: Option<String>,
) -> Option<String> {
    if opened_count == 1 && duplicate_count == 0 && failure_count == 0 {
        return last_artifact_warning;
    }

    let mut parts = Vec::new();

    if opened_count > 0 {
        if artifact_count > 0 {
            parts.push(format!(
                "Opened {} ({} with formatting artifacts)",
                file_count_label(opened_count),
                file_count_label(artifact_count)
            ));
        } else {
            parts.push(format!("Opened {}", file_count_label(opened_count)));
        }
    }

    if duplicate_count > 0 {
        parts.push(format!(
            "{} already open",
            file_count_label(duplicate_count)
        ));
    }

    if failure_count > 0 {
        parts.push(format!(
            "{} failed to open",
            file_count_label(failure_count)
        ));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

fn file_count_label(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

#[cfg(test)]
mod tests {
    use super::summarize_open_results;

    #[test]
    fn summarize_open_results_preserves_single_artifact_warning() {
        let summary = summarize_open_results(
            1,
            0,
            0,
            1,
            Some("Opened with formatting artifacts: Control characters present: ANSI".to_owned()),
        );

        assert_eq!(
            summary,
            Some("Opened with formatting artifacts: Control characters present: ANSI".to_owned())
        );
    }

    #[test]
    fn summarize_open_results_aggregates_batch_outcomes() {
        let summary = summarize_open_results(3, 1, 2, 1, None);

        assert_eq!(
            summary,
            Some("Opened 3 files (1 file with formatting artifacts); 1 file already open; 2 files failed to open".to_owned())
        );
    }
}
