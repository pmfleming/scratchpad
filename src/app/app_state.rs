use crate::app::chrome::handle_window_resize;
use crate::app::commands::AppCommand;
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::file_service::FileService;
use crate::app::services::session_store::SessionStore;
use crate::app::ui::{dialogs, editor_area, tab_strip};
use crate::app::{paths_match, theme};
use eframe::egui;
use std::path::Path;
use std::time::{Duration, Instant};

const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);

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

impl Default for ScratchpadApp {
    fn default() -> Self {
        Self::with_session_store(SessionStore::default())
    }
}

impl eframe::App for ScratchpadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().close_requested()) && !self.close_in_progress {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_exit(ctx);
            return;
        }

        handle_window_resize(ctx);
        self.maybe_persist_session(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        tab_strip::show_header(ctx, self);
        editor_area::show_status_bar(ctx, self);
        editor_area::show_editor(ctx, self);
        dialogs::show_pending_action_modal(ctx, self);
        self.handle_shortcuts(ctx);
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

        app.restore_session_state();

        app
    }

    pub(crate) fn active_tab(&self) -> Option<&WorkspaceTab> {
        self.tabs.get(self.active_tab_index)
    }

    pub(crate) fn mark_session_dirty(&mut self) {
        self.session_dirty = true;
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
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.open_selected_path(path);
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

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        self.handle_file_shortcuts(ctx);
        self.handle_view_shortcuts(ctx);
        self.handle_tab_shortcuts(ctx);
    }

    fn handle_file_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.handle_command(AppCommand::NewTab);
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.handle_command(AppCommand::OpenFile);
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.handle_command(AppCommand::SaveFile);
        }
    }

    fn handle_view_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::CTRL, egui::Key::Equals)
                || input.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
        }) {
            self.font_size = (self.font_size + 1.0).min(72.0);
            self.mark_session_dirty();
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Minus)) {
            self.font_size = (self.font_size - 1.0).max(8.0);
            self.mark_session_dirty();
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Num0)) {
            self.font_size = 14.0;
            self.mark_session_dirty();
        }
    }

    fn handle_tab_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::W))
            && !self.tabs.is_empty()
        {
            self.handle_command(AppCommand::RequestCloseTab {
                index: self.active_tab_index,
            });
        }
    }

    fn maybe_persist_session(&mut self, ctx: &egui::Context) {
        if !self.session_dirty {
            return;
        }

        ctx.request_repaint_after(SESSION_SNAPSHOT_INTERVAL);
        if self.last_session_persist.elapsed() < SESSION_SNAPSHOT_INTERVAL {
            return;
        }

        if let Err(error) = self.persist_session_now() {
            self.status_message = Some(format!("Session save failed: {error}"));
        }
    }

    fn restore_session_state(&mut self) {
        match self.session_store.load() {
            Ok(Some(restored)) => self.apply_restored_session(restored),
            Ok(None) => {}
            Err(error) => {
                self.status_message = Some(format!("Session restore failed: {error}"));
            }
        }
    }

    fn apply_restored_session(
        &mut self,
        restored: crate::app::services::session_store::RestoredSession,
    ) {
        self.tabs = restored.tabs;
        self.active_tab_index = restored.active_tab_index;
        self.font_size = restored.font_size;
        self.word_wrap = restored.word_wrap;
    }

    fn open_selected_path(&mut self, path: std::path::PathBuf) {
        if self.activate_existing_path(&path) {
            return;
        }

        match FileService::read_file(&path) {
            Ok(file_content) => self.open_loaded_file(path, file_content),
            Err(error) => {
                self.status_message = Some(format!("Open failed: {error}"));
            }
        }
    }

    fn activate_existing_path(&mut self, path: &Path) -> bool {
        if let Some(index) = self.find_tab_by_path(path) {
            self.handle_command(AppCommand::ActivateTab { index });
            self.status_message = Some(format!(
                "{} is already open.",
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            ));
            true
        } else {
            false
        }
    }

    fn open_loaded_file(
        &mut self,
        path: std::path::PathBuf,
        file_content: crate::app::services::file_service::FileContent,
    ) {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        self.append_tab(WorkspaceTab::new(BufferState::with_encoding(
            name,
            file_content.content,
            Some(path),
            file_content.encoding,
            file_content.has_bom,
        )));
        self.status_message = None;
        let _ = self.persist_session_now();
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

    fn persist_session_now(&mut self) -> std::io::Result<()> {
        self.session_store.persist(
            &self.tabs,
            self.active_tab_index,
            self.font_size,
            self.word_wrap,
        )?;
        self.session_dirty = false;
        self.last_session_persist = Instant::now();
        Ok(())
    }
}
