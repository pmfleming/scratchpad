use crate::app::chrome::*;
use crate::app::session::SessionStore;
use crate::app::tabs::TabState;
use crate::app::theme::*;
use eframe::egui::{self, Sense, Stroke};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy)]
enum PendingAction {
    CloseTab(usize),
}

pub struct ScratchpadApp {
    pub tabs: Vec<TabState>,
    pub active_tab_index: usize,
    pending_action: Option<PendingAction>,
    pub icons: Option<AppIcons>,
    pub font_size: f32,
    pub word_wrap: bool,
    pub status_message: Option<String>,
    session_store: SessionStore,
    session_dirty: bool,
    last_session_persist: Instant,
    close_in_progress: bool,
}

impl Default for ScratchpadApp {
    fn default() -> Self {
        let session_store = SessionStore::default();
        let mut app = Self {
            tabs: vec![TabState::new("Untitled".to_owned(), String::new(), None)],
            active_tab_index: 0,
            pending_action: None,
            icons: None,
            font_size: 14.0,
            word_wrap: true,
            status_message: None,
            session_store,
            session_dirty: false,
            last_session_persist: Instant::now(),
            close_in_progress: false,
        };

        match app.session_store.load() {
            Ok(Some(restored)) => {
                app.tabs = restored.tabs;
                app.active_tab_index = restored.active_tab_index;
                app.font_size = restored.font_size;
                app.word_wrap = restored.word_wrap;
            }
            Ok(None) => {}
            Err(error) => {
                app.status_message = Some(format!("Session restore failed: {error}"));
            }
        }

        app
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

        // Ensure icons are loaded. We do this in a way that doesn't keep a mutable borrow of self alive.
        if self.icons.is_none() {
            self.icons = Some(AppIcons::load(ctx));
        }

        // Clone handles to avoid borrowing self.icons later
        let icons = self.icons.as_ref().unwrap();
        let close_icon = icons.close.clone();
        let min_icon = icons.minimize.clone();
        let max_icon = icons.maximize.clone();
        let open_file_icon = icons.open_file.clone();
        let save_icon = icons.save.clone();
        let search_icon = icons.search.clone();
        let new_tab_icon = icons.new_tab.clone();

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        // Top Panel
        egui::TopBottomPanel::top("header")
            .exact_height(HEADER_HEIGHT)
            .frame(
                egui::Frame::none()
                    .fill(HEADER_BG)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if icon_button(
                        ui,
                        &open_file_icon,
                        BUTTON_SIZE,
                        ACTION_BG,
                        ACTION_HOVER_BG,
                        "Open File",
                    )
                    .clicked()
                    {
                        self.open_file();
                    }
                    if icon_button(
                        ui,
                        &save_icon,
                        BUTTON_SIZE,
                        ACTION_BG,
                        ACTION_HOVER_BG,
                        "Save As",
                    )
                    .clicked()
                    {
                        self.save_file_as();
                    }
                    if icon_button(
                        ui,
                        &search_icon,
                        BUTTON_SIZE,
                        ACTION_BG,
                        ACTION_HOVER_BG,
                        "Search",
                    )
                    .clicked()
                    {
                        self.status_message = Some("Search is not implemented yet.".to_owned());
                    }

                    ui.add_space(8.0);

                    // 2. Right-side Buttons (Right-to-Left)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close Window
                        if icon_button(
                            ui,
                            &close_icon,
                            CAPTION_BUTTON_SIZE,
                            CLOSE_BG,
                            CLOSE_HOVER_BG,
                            "Close",
                        )
                        .clicked()
                        {
                            self.request_exit(ctx);
                        }

                        // Maximize / Restore
                        let maximized =
                            ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                        if maximized {
                            if restore_button(
                                ui,
                                CAPTION_BUTTON_SIZE,
                                ACTION_BG,
                                ACTION_HOVER_BG,
                                "Restore",
                            )
                            .clicked()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                            }
                        } else {
                            if icon_button(
                                ui,
                                &max_icon,
                                CAPTION_BUTTON_SIZE,
                                ACTION_BG,
                                ACTION_HOVER_BG,
                                "Maximize",
                            )
                            .clicked()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
                            }
                        }

                        // Minimize
                        if icon_button(
                            ui,
                            &min_icon,
                            CAPTION_BUTTON_SIZE,
                            ACTION_BG,
                            ACTION_HOVER_BG,
                            "Minimize",
                        )
                        .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }

                        ui.add_space(8.0);

                        // 3. Middle Area (Tab Strip & Drag Area)
                        let remaining_width = ui.available_width();

                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.set_max_width(remaining_width);

                            // Drag handle (flexible space)
                            let drag_response = ui.interact(
                                ui.available_rect_before_wrap(),
                                ui.id().with("drag_area"),
                                Sense::click_and_drag(),
                            );
                            if drag_response.drag_started() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }
                            if drag_response.double_clicked() {
                                let maximized =
                                    ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                            }

                            // Tab strip
                            egui::ScrollArea::horizontal()
                                .id_source("tab_strip")
                                .auto_shrink([true, false])
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let mut activated_tab = None;
                                        for (i, tab) in self.tabs.iter().enumerate() {
                                            let is_active = self.active_tab_index == i;
                                            let mut closed = false;
                                            let mut clicked = false;

                                            ui.push_id(i, |ui| {
                                                let (tab_response, close_response) = tab_button(
                                                    ui,
                                                    &tab.display_name(),
                                                    is_active,
                                                    &close_icon,
                                                );

                                                if tab_response.clicked() {
                                                    clicked = true;
                                                }

                                                if close_response.clicked() {
                                                    closed = true;
                                                }
                                            });

                                            if clicked {
                                                activated_tab = Some(i);
                                            }
                                            if closed {
                                                self.pending_action =
                                                    Some(PendingAction::CloseTab(i));
                                            }
                                        }

                                        if let Some(index) = activated_tab {
                                            self.active_tab_index = index;
                                            self.mark_session_dirty();
                                        }

                                        ui.add_space(4.0);
                                        if icon_button(
                                            ui,
                                            &new_tab_icon,
                                            BUTTON_SIZE,
                                            ACTION_BG,
                                            ACTION_HOVER_BG,
                                            "New Tab",
                                        )
                                        .clicked()
                                        {
                                            self.new_tab();
                                        }
                                    });
                                });
                        });
                    });
                });
            });

        // Bottom Panel
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if !self.tabs.is_empty() {
                    let tab = &self.tabs[self.active_tab_index];
                    ui.label(format!(
                        "Path: {}",
                        tab.path
                            .as_ref()
                            .map(|p| p.to_string_lossy())
                            .unwrap_or_else(|| "Untitled".into())
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("Lines: {}", tab.content.lines().count()));
                    });
                }
                if let Some(message) = &self.status_message {
                    ui.separator();
                    ui.label(message);
                }
            });
        });

        // Central Panel
        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.tabs.is_empty() {
                let panel_rect = ui.max_rect();
                let pointer_over_editor = ui.rect_contains_pointer(panel_rect);
                let zoom_delta = ctx.input_mut(|input| {
                    if pointer_over_editor && input.modifiers.ctrl {
                        let delta = input.raw_scroll_delta.y;
                        if delta != 0.0 {
                            input.raw_scroll_delta = egui::Vec2::ZERO;
                            input.smooth_scroll_delta = egui::Vec2::ZERO;
                        }
                        delta
                    } else {
                        0.0
                    }
                });
                if zoom_delta != 0.0 {
                    self.font_size = (self.font_size + zoom_delta * 0.05).clamp(8.0, 72.0);
                    self.mark_session_dirty();
                }

                self.active_tab_index = self.active_tab_index.min(self.tabs.len() - 1);
                let tab = &mut self.tabs[self.active_tab_index];
                let font_id = egui::FontId::monospace(self.font_size);
                let editor_font_id = font_id.clone();
                let text_color = ui.visuals().text_color();
                let word_wrap = self.word_wrap;
                let line_count = tab.content.lines().count().max(1);
                let mut layouter = move |ui: &egui::Ui, text: &str, wrap_width: f32| {
                    let mut job = egui::text::LayoutJob::default();
                    job.wrap.max_width = if word_wrap { wrap_width } else { f32::INFINITY };
                    job.append(
                        text,
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color: text_color,
                            ..Default::default()
                        },
                    );
                    ui.fonts(|fonts| fonts.layout_job(job))
                };

                let mut editor_changed = false;
                egui::ScrollArea::both()
                    .id_source(("editor_scroll", self.active_tab_index))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let editor = egui::TextEdit::multiline(&mut tab.content)
                            .font(editor_font_id)
                            .desired_width(if word_wrap {
                                ui.available_width()
                            } else {
                                f32::INFINITY
                            })
                            .desired_rows(line_count)
                            .lock_focus(true)
                            .layouter(&mut layouter);

                        if ui.add(editor).changed() {
                            tab.is_dirty = true;
                            self.status_message = None;
                            editor_changed = true;
                        }
                    });

                if editor_changed {
                    self.mark_session_dirty();
                }
            }
        });

        self.show_pending_action_modal(ctx);

        // Shortcuts
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
            self.new_tab();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.open_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::W))
            && !self.tabs.is_empty()
        {
            self.pending_action = Some(PendingAction::CloseTab(self.active_tab_index));
        }
    }
}

impl Drop for ScratchpadApp {
    fn drop(&mut self) {
        let _ = self.persist_session_now();
    }
}

impl ScratchpadApp {
    fn window_title(&self) -> String {
        if self.tabs.is_empty() {
            return "Scratchpad".to_owned();
        }

        let tab = &self.tabs[self.active_tab_index.min(self.tabs.len() - 1)];
        let marker = if tab.is_dirty { "*" } else { "" };
        format!("{}{} - Scratchpad", marker, tab.name)
    }

    pub fn new_tab(&mut self) {
        self.tabs
            .push(TabState::new("Untitled".to_owned(), String::new(), None));
        self.active_tab_index = self.tabs.len() - 1;
        let _ = self.persist_session_now();
    }

    pub fn open_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            if let Some(index) = self.find_tab_by_path(&path) {
                self.active_tab_index = index;
                self.status_message = Some(format!(
                    "{} is already open.",
                    path.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string())
                ));
                self.mark_session_dirty();
                return;
            }

            match fs::read_to_string(&path) {
                Ok(content) => {
                    let name = path.file_name().unwrap().to_string_lossy().into_owned();
                    self.tabs.push(TabState::new(name, content, Some(path)));
                    self.active_tab_index = self.tabs.len() - 1;
                    self.status_message = None;
                    let _ = self.persist_session_now();
                }
                Err(error) => {
                    self.status_message = Some(format!("Open failed: {error}"));
                }
            }
        }
    }

    pub fn save_file(&mut self) {
        let _ = self.save_file_at(self.active_tab_index);
    }

    pub fn save_file_at(&mut self, index: usize) -> bool {
        if self.tabs.is_empty() {
            return false;
        }
        if self.tabs[index].path.is_some() {
            let tab = &mut self.tabs[index];
            let path = tab.path.clone().unwrap();
            match fs::write(&path, &tab.content) {
                Ok(()) => {
                    tab.is_dirty = false;
                    self.status_message = None;
                    self.mark_session_dirty();
                    let _ = self.persist_session_now();
                    true
                }
                Err(error) => {
                    self.status_message = Some(format!("Save failed: {error}"));
                    false
                }
            }
        } else {
            self.save_file_as_at(index);
            !self.tabs[index].is_dirty
        }
    }

    pub fn save_file_as(&mut self) {
        let _ = self.save_file_as_at(self.active_tab_index);
    }

    pub fn save_file_as_at(&mut self, index: usize) -> bool {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&self.tabs[index].name)
            .save_file()
        {
            let tab = &mut self.tabs[index];
            match fs::write(&path, &tab.content) {
                Ok(()) => {
                    tab.path = Some(path.clone());
                    tab.name = path.file_name().unwrap().to_string_lossy().into_owned();
                    tab.is_dirty = false;
                    self.status_message = None;
                    self.mark_session_dirty();
                    let _ = self.persist_session_now();
                    true
                }
                Err(error) => {
                    self.status_message = Some(format!("Save failed: {error}"));
                    false
                }
            }
        } else {
            self.status_message = Some("Save cancelled.".to_owned());
            false
        }
    }

    pub fn perform_close_tab(&mut self, index: usize) {
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.tabs
                .push(TabState::new("Untitled".to_owned(), String::new(), None));
            self.active_tab_index = 0;
            let _ = self.persist_session_now();
            return;
        }

        if self.active_tab_index > index {
            self.active_tab_index -= 1;
        }
        self.active_tab_index = self.active_tab_index.min(self.tabs.len() - 1);
        let _ = self.persist_session_now();
    }

    fn request_exit(&mut self, ctx: &egui::Context) {
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

    fn show_pending_action_modal(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_action else {
            return;
        };

        match action {
            PendingAction::CloseTab(index) => {
                if index >= self.tabs.len() {
                    self.pending_action = None;
                    return;
                }

                let is_dirty = self.tabs[index].is_dirty;
                let tab_name = self.tabs[index].name.clone();

                if !is_dirty {
                    self.perform_close_tab(index);
                    self.pending_action = None;
                    return;
                }

                egui::Window::new("Unsaved Changes")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Do you want to save changes to {}?", tab_name));
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() && self.save_file_at(index) {
                                self.perform_close_tab(index);
                                self.pending_action = None;
                            }
                            if ui.button("Don't Save").clicked() {
                                self.perform_close_tab(index);
                                self.pending_action = None;
                            }
                            if ui.button("Cancel").clicked() {
                                self.pending_action = None;
                            }
                        });
                    });
            }
        }
    }

    fn mark_session_dirty(&mut self) {
        self.session_dirty = true;
    }

    fn find_tab_by_path(&self, candidate: &Path) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            tab.path
                .as_deref()
                .is_some_and(|path| paths_match(path, candidate))
        })
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

fn paths_match(left: &Path, right: &Path) -> bool {
    normalize_path(left) == normalize_path(right)
}

fn normalize_path(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::paths_match;
    use std::path::Path;

    #[test]
    fn path_match_is_case_insensitive_on_windows_paths() {
        assert!(paths_match(
            Path::new(r"C:\Temp\notes.txt"),
            Path::new(r"c:\temp\NOTES.txt")
        ));
    }

    #[test]
    fn path_match_rejects_different_files() {
        assert!(!paths_match(
            Path::new(r"C:\Temp\notes.txt"),
            Path::new(r"C:\Temp\other.txt")
        ));
    }
}

pub mod chrome;
pub mod session;
pub mod tabs;
pub mod theme;
