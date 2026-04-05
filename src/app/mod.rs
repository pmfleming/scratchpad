use crate::app::chrome::*;
use crate::app::session::SessionStore;
use crate::app::tabs::TabState;
use crate::app::theme::*;
use eframe::egui::{self, Sense, Stroke};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);
const OVERFLOW_CLOSE_BUTTON_WIDTH: f32 = 22.0;

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
    pending_scroll_to_active: bool,
    overflow_popup_open: bool,
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
            pending_scroll_to_active: true,
            overflow_popup_open: false,
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
                    let spacing = ui.spacing().item_spacing.x;
                    let caption_controls_width = CAPTION_BUTTON_SIZE.x * 3.0 + spacing * 2.0;
                    let tab_action_width = BUTTON_SIZE.x;
                    let overflow_button_width = BUTTON_SIZE.x;
                    let spacer_before_captions = 8.0;
                    let remaining_width = ui.available_width();
                    let viewport_width_with_overflow = (remaining_width
                        - caption_controls_width
                        - spacer_before_captions
                        - tab_action_width
                        - spacing
                        - overflow_button_width
                        - spacing)
                        .max(0.0);
                    let total_tab_width = self.estimated_tab_strip_width(spacing);
                    let has_overflow = total_tab_width > viewport_width_with_overflow;
                    let viewport_width = (remaining_width
                        - caption_controls_width
                        - spacer_before_captions
                        - tab_action_width
                        - spacing
                        - if has_overflow {
                            overflow_button_width + spacing
                        } else {
                            0.0
                        })
                    .max(0.0);
                    let visible_strip_width = total_tab_width.min(viewport_width);
                    let drag_width = (viewport_width - visible_strip_width).max(0.0);
                    let duplicate_name_counts = duplicate_name_counts(&self.tabs);
                    let mut activated_tab = None;
                    let mut overflow_activated_tab = None;
                    let mut overflow_closed_tab = None;
                    let mut consumed_scroll_request = false;

                    let tab_area_width = (remaining_width - caption_controls_width - spacer_before_captions).max(0.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(tab_area_width, TAB_HEIGHT),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            if visible_strip_width > 0.0 {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(visible_strip_width, TAB_HEIGHT),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.set_width(visible_strip_width);
                                        ui.set_min_width(visible_strip_width);
                                        ui.set_max_width(visible_strip_width);

                                        egui::ScrollArea::horizontal()
                                            .id_source("tab_strip")
                                            .auto_shrink([false, false])
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    for (i, tab) in self.tabs.iter().enumerate() {
                                                        let is_active = self.active_tab_index == i;
                                                        let mut closed = false;
                                                        let mut clicked = false;

                                                        ui.push_id(i, |ui| {
                                                            let (
                                                                tab_response,
                                                                close_response,
                                                                truncated,
                                                            ) = tab_button(
                                                                ui,
                                                                &tab.display_name(),
                                                                is_active,
                                                                &close_icon,
                                                            );

                                                            let tab_response = if truncated {
                                                                tab_response.on_hover_text(
                                                                    tab.display_name(),
                                                                )
                                                            } else {
                                                                tab_response
                                                            };

                                                            if is_active
                                                                && self.pending_scroll_to_active
                                                            {
                                                                tab_response.scroll_to_me(Some(
                                                                    egui::Align::Center,
                                                                ));
                                                                consumed_scroll_request = true;
                                                            }

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
                                                });
                                            });
                                    },
                                );
                            }

                            if has_overflow || self.overflow_popup_open {
                                ui.add_space(spacing);
                                let overflow_popup_id = ui.id().with("tab_overflow_popup");
                                let overflow_button_response = ui.add_sized(
                                    [BUTTON_SIZE.x, BUTTON_SIZE.y],
                                    egui::Button::new(egui::RichText::new("v").color(TEXT_PRIMARY))
                                        .fill(ACTION_BG)
                                        .stroke(Stroke::new(1.0, BORDER)),
                                );

                                if overflow_button_response.clicked() {
                                    self.overflow_popup_open = !self.overflow_popup_open;
                                }

                                if self.overflow_popup_open {
                                    let popup_width = TAB_BUTTON_WIDTH + OVERFLOW_CLOSE_BUTTON_WIDTH;
                                    let area_response = egui::Area::new(overflow_popup_id)
                                        .order(egui::Order::Foreground)
                                        .constrain(true)
                                        .fixed_pos(overflow_button_response.rect.right_bottom())
                                        .pivot(egui::Align2::RIGHT_TOP)
                                        .show(ctx, |ui| {
                                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                                ui.set_width(popup_width);
                                                ui.set_min_width(popup_width);

                                                for (i, tab) in self.tabs.iter().enumerate() {
                                                    let label = tab.display_name();
                                                    let selected = self.active_tab_index == i;

                                                    ui.allocate_ui_with_layout(
                                                        egui::vec2(popup_width, TAB_HEIGHT),
                                                        egui::Layout::left_to_right(egui::Align::Center),
                                                        |ui| {
                                                            ui.allocate_ui_with_layout(
                                                                egui::vec2(
                                                                    popup_width
                                                                        - OVERFLOW_CLOSE_BUTTON_WIDTH,
                                                                    TAB_HEIGHT,
                                                                ),
                                                                egui::Layout::left_to_right(
                                                                    egui::Align::Center,
                                                                ),
                                                                |ui| {
                                                                    ui.set_width(
                                                                        popup_width
                                                                            - OVERFLOW_CLOSE_BUTTON_WIDTH,
                                                                    );
                                                                    ui.vertical(|ui| {
                                                                        let response = ui.selectable_label(
                                                                            selected,
                                                                            &label,
                                                                        );
                                                                        if response.clicked() {
                                                                            overflow_activated_tab =
                                                                                Some(i);
                                                                            self.overflow_popup_open =
                                                                                false;
                                                                        }

                                                                        if duplicate_name_counts
                                                                            .get(&tab.name)
                                                                            .copied()
                                                                            .unwrap_or(0)
                                                                            > 1
                                                                        {
                                                                            ui.label(
                                                                                egui::RichText::new(
                                                                                    tab.overflow_context_label(),
                                                                                )
                                                                                .small()
                                                                                .color(TEXT_MUTED),
                                                                            );
                                                                        }
                                                                    });
                                                                },
                                                            );

                                                            let close_response = ui
                                                                .add_sized(
                                                                    [OVERFLOW_CLOSE_BUTTON_WIDTH, 22.0],
                                                                    egui::Button::new(
                                                                        egui::RichText::new("×")
                                                                            .color(TEXT_MUTED),
                                                                    )
                                                                    .fill(ACTION_BG)
                                                                    .stroke(Stroke::new(1.0, BORDER)),
                                                                )
                                                                .on_hover_text("Close Tab");

                                                            if close_response.clicked() {
                                                                overflow_closed_tab = Some(i);
                                                            }
                                                        },
                                                    );
                                                }
                                            });
                                        });

                                    if ctx.input(|i| i.key_pressed(egui::Key::Escape))
                                        || (overflow_button_response.clicked_elsewhere()
                                            && !area_response.response.hovered()
                                            && overflow_closed_tab.is_none())
                                    {
                                        self.overflow_popup_open = false;
                                    }
                                }
                            }

                            ui.add_space(spacing);
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

                            if drag_width > 0.0 {
                                let (rect, drag_response) = ui.allocate_exact_size(
                                    egui::vec2(drag_width, TAB_HEIGHT),
                                    Sense::click_and_drag(),
                                );
                                if drag_response.drag_started() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                                }
                                if drag_response.double_clicked() {
                                    let maximized = ctx
                                        .input(|input| input.viewport().maximized.unwrap_or(false));
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                                        !maximized,
                                    ));
                                }
                                ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                            }
                        },
                    );

                    ui.add_space(8.0);

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

                    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
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
                    } else if icon_button(
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

                    if let Some(index) = activated_tab.or(overflow_activated_tab) {
                        self.active_tab_index = index;
                        self.pending_scroll_to_active = true;
                        self.mark_session_dirty();
                    }

                    if let Some(index) = overflow_closed_tab {
                        self.pending_action = Some(PendingAction::CloseTab(index));
                    }

                    if consumed_scroll_request {
                        self.pending_scroll_to_active = false;
                    }
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
                let zoom_factor = ctx.input(|i| i.zoom_delta());
                if pointer_over_editor && zoom_factor != 1.0 {
                    self.font_size = (self.font_size * zoom_factor).clamp(8.0, 72.0);
                    self.mark_session_dirty();
                }

                self.active_tab_index = self.active_tab_index.min(self.tabs.len() - 1);
                let tab = &mut self.tabs[self.active_tab_index];
                let font_id = egui::FontId::monospace(self.font_size);
                let editor_font_id = font_id.clone();
                let text_color = TEXT_PRIMARY;
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
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL, egui::Key::Equals)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
        }) {
            self.font_size = (self.font_size + 1.0).min(72.0);
            self.mark_session_dirty();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Minus)) {
            self.font_size = (self.font_size - 1.0).max(8.0);
            self.mark_session_dirty();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Num0)) {
            self.font_size = 14.0;
            self.mark_session_dirty();
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
        self.pending_scroll_to_active = true;
        let _ = self.persist_session_now();
    }

    pub fn open_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            if let Some(index) = self.find_tab_by_path(&path) {
                self.active_tab_index = index;
                self.pending_scroll_to_active = true;
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
                    self.pending_scroll_to_active = true;
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
        self.pending_scroll_to_active = true;
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

    fn estimated_tab_strip_width(&self, spacing: f32) -> f32 {
        if self.tabs.is_empty() {
            return 0.0;
        }

        (self.tabs.len() as f32 * TAB_BUTTON_WIDTH)
            + ((self.tabs.len().saturating_sub(1)) as f32 * spacing)
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

fn duplicate_name_counts(tabs: &[TabState]) -> HashMap<String, usize> {
    let mut counts = HashMap::with_capacity(tabs.len());
    for tab in tabs {
        *counts.entry(tab.name.clone()).or_insert(0) += 1;
    }
    counts
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
