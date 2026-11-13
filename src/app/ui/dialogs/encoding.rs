//! Reopen or save the active file with another encoding, in History's panel style.
use crate::app::app_state::{EncodingMode, ScratchpadApp, frame, workspace};
use crate::app::commands::{AppCommand, FileCommand};
use crate::app::domain::EncodingSource;
use crate::app::services::file_service::{COMMON_TEXT_ENCODINGS, EncodingOption, FileService};
use crate::app::theme;
use crate::app::ui::{panel_style, panel_style::window, widget_ids};
use eframe::egui;

const ENCODING_PANEL_WIDTH: f32 = 400.0;
const ROW_HEIGHT: f32 = panel_style::ROW_HEIGHT;
const LIST_MAX_HEIGHT: f32 = 12.0 * ROW_HEIGHT;
const NOTE_SIZE: f32 = 12.0;
const HINT_SIZE: f32 = 11.0;
const KEY_HINTS: &str = "↑ ↓ Move   Enter Apply   Esc Close";
const CURRENT: &str = "CURRENT";

#[cfg(test)]
mod tests;

/// What the panel knows about the active file.
struct FileFacts {
    tab_index: usize,
    name: String,
    path: Option<String>,
    encoding: String,
    label: String,
    source: EncodingSource,
    dirty: bool,
}

impl FileFacts {
    fn from_app(app: &ScratchpadApp) -> Self {
        let tab_index = app.tab_manager.active_tab_index;
        match app.tab_manager.active_tab() {
            Some(tab) => {
                let buffer = tab.active_buffer();
                Self {
                    tab_index,
                    name: buffer.name.clone(),
                    path: buffer.path.as_ref().map(|p| p.display().to_string()),
                    encoding: buffer.format.encoding_name.clone(),
                    label: buffer.format.encoding_label(),
                    source: buffer.format.encoding_source,
                    dirty: buffer.is_dirty,
                }
            }
            None => Self {
                tab_index,
                name: "Untitled".into(),
                path: None,
                encoding: "UTF-8".into(),
                label: "UTF-8".into(),
                source: EncodingSource::DefaultForNewFile,
                dirty: false,
            },
        }
    }

    /// Why Reopen is unavailable, if it is.
    fn reopen_blocked(&self) -> Option<&'static str> {
        if self.path.is_none() {
            Some("Save this file before reopening it with another encoding.")
        } else if self.dirty {
            Some("Save or discard changes before reopening with another encoding.")
        } else {
            None
        }
    }
}

#[derive(Default)]
struct Actions {
    close: bool,
    apply: bool,
}

fn list_focus_id() -> egui::Id {
    egui::Id::unique("encoding.list.focus")
}

pub(crate) fn show_encoding_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.state.dialogs.encoding.is_open() {
        return;
    }
    let facts = FileFacts::from_app(app);
    let mut actions = Actions::default();
    let layer = widget_ids::area_layer_id(egui::Order::Foreground, "encoding_window");
    let outside = window::OutsideClick::before_show(ctx, app, layer);
    let frame = panel_style::window_frame(ctx, app.state.app_settings.editor_background_color());
    let outer_width = ENCODING_PANEL_WIDTH + frame.total_margin().sum().x;
    let response = egui::Window::new("Encoding")
        .id(layer.id)
        .order(layer.order)
        .title_bar(false)
        .resizable(false)
        .frame(frame)
        .default_pos(window::top_right(ctx.content_rect(), outer_width))
        .show(ctx, |ui| {
            window::contents(ui, "encoding_dialog", frame.fill, |ui| {
                ui.set_width(ENCODING_PANEL_WIDTH);
                actions.close |= header(ui, app, outside.needs_close_button());
                file_line(ui, &facts);
                ui.separator();
                list(ui, app, &facts, &mut actions);
                ui.separator();
                footer(ui, app, &facts, &mut actions);
            });
        });
    if let Some(response) = response {
        // Ignore the opening click and interactions with menus.
        actions.close |= outside.after_show(ctx, app, &response.response);
    }
    actions.close |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if actions.apply && apply(app, &facts) {
        actions.close = true;
    }
    if actions.close {
        frame::close_encoding_dialog(app);
        workspace::accessors::request_focus_for_active_view(app);
    }
}

/// Title on the left; the Reopen │ Save switch on the right. Returns true on close.
fn header(ui: &mut egui::Ui, app: &mut ScratchpadApp, show_close: bool) -> bool {
    let close = show_close.then_some(("encoding.close", "Close encoding"));
    window::header(ui, "Encoding", close, |switch| {
        let mode = &mut app.state.dialogs.encoding.mode;
        switch
            .ui
            .selectable_value(mode, EncodingMode::Save, "Save")
            .on_hover_text("Write the file with the selected encoding");
        switch
            .ui
            .selectable_value(mode, EncodingMode::Reopen, "Reopen")
            .on_hover_text("Read the file from disk again with the selected encoding");
    })
    .close
}

fn file_line(ui: &mut egui::Ui, facts: &FileFacts) {
    let source = match facts.source {
        EncodingSource::Bom => "from its byte order mark",
        EncodingSource::Heuristic => "detected",
        EncodingSource::ExplicitUserChoice => "chosen",
        EncodingSource::DefaultForNewFile => "default",
    };
    let text = format!("{} · {} ({source})", facts.name, facts.label);
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), 18.0));
    panel_style::text_end(ui, rect, &text, NOTE_SIZE, theme::text_muted(ui));
    let response = widget_ids::interact(
        ui,
        rect,
        egui::Id::unique("encoding.file"),
        egui::Sense::hover(),
        "encoding-file",
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &text));
    response.on_hover_text(facts.path.as_deref().unwrap_or(&facts.name));
}

fn region(option: &EncodingOption) -> &'static str {
    match option.canonical_name {
        "UTF-8" | "UTF-16LE" | "UTF-16BE" => "Unicode",
        "windows-1252" => "Western European",
        "windows-1251" => "Cyrillic",
        "windows-1250" => "Central European",
        "Shift_JIS" | "EUC-JP" => "Japanese",
        "GBK" => "Simplified Chinese",
        "Big5" => "Traditional Chinese",
        "EUC-KR" => "Korean",
        _ => "",
    }
}

fn selected_index(choice: &str) -> Option<usize> {
    COMMON_TEXT_ENCODINGS
        .iter()
        .position(|option| option.canonical_name.eq_ignore_ascii_case(choice))
}

fn list(ui: &mut egui::Ui, app: &mut ScratchpadApp, facts: &FileFacts, actions: &mut Actions) {
    keyboard(ui, app, actions);
    let selected = selected_index(&app.state.dialogs.encoding.choice);
    let output = egui::ScrollArea::vertical()
        .id_salt("encoding.list")
        .max_height(LIST_MAX_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, option) in COMMON_TEXT_ENCODINGS.iter().enumerate() {
                let current = option.canonical_name.eq_ignore_ascii_case(&facts.encoding);
                let row = row(ui, option, Some(index) == selected, current);
                if row.clicked() || row.double_clicked() {
                    option
                        .canonical_name
                        .clone_into(&mut app.state.dialogs.encoding.choice);
                    ui.memory_mut(|m| m.request_focus(list_focus_id()));
                }
                actions.apply |= row.double_clicked();
            }
        });
    let focus = widget_ids::interact(
        ui,
        output.inner_rect,
        list_focus_id(),
        egui::Sense::focusable_noninteractive(),
        "encoding-list-focus",
    );
    focus.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ListBox, true, "Encodings"));
    if std::mem::take(&mut app.state.dialogs.encoding.focus_pending) {
        focus.request_focus();
    }
    if focus.has_focus() {
        panel_style::lock_navigation_focus(ui.ctx(), list_focus_id());
    }
}

fn keyboard(ui: &egui::Ui, app: &mut ScratchpadApp, actions: &mut Actions) {
    if !ui.memory(|m| m.has_focus(list_focus_id())) {
        return;
    }
    let consume = |key| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
    let last = COMMON_TEXT_ENCODINGS.len() - 1;
    let current = selected_index(&app.state.dialogs.encoding.choice);
    let mut target = None;
    if consume(egui::Key::ArrowDown) {
        target = Some(current.map_or(0, |i| (i + 1).min(last)));
    }
    if consume(egui::Key::ArrowUp) {
        target = Some(current.map_or(0, |i| i.saturating_sub(1)));
    }
    if consume(egui::Key::Home) {
        target = Some(0);
    }
    if consume(egui::Key::End) {
        target = Some(last);
    }
    if let Some(index) = target {
        COMMON_TEXT_ENCODINGS[index]
            .canonical_name
            .clone_into(&mut app.state.dialogs.encoding.choice);
    }
    actions.apply |= consume(egui::Key::Enter);
}

/// A flat row: label, region and the file's current encoding tag.
fn row(
    ui: &mut egui::Ui,
    option: &EncodingOption,
    selected: bool,
    current: bool,
) -> egui::Response {
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), ROW_HEIGHT));
    let response = widget_ids::interact(
        ui,
        rect,
        egui::Id::unique(("encoding.row", option.canonical_name)),
        egui::Sense::click(),
        "encoding-row",
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, selected, option.label)
    });
    if selected {
        ui.scroll_to_rect(rect, None);
    }
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            0,
            if selected {
                theme::tab_selected_bg(ui)
            } else {
                theme::action_bg(ui)
            },
        );
    }
    let inner = rect.shrink2(egui::vec2(10.0, 0.0));
    let mut right = inner.right();
    if current {
        let width = panel_style::badge_width(ui, CURRENT);
        let tag = egui::Rect::from_center_size(
            egui::pos2(right - width * 0.5, inner.center().y),
            egui::vec2(width, 17.0),
        );
        panel_style::badge(ui, tag, CURRENT);
        right = tag.left() - 8.0;
    }
    let region_width = 140.0_f32.min((right - inner.left()) * 0.5);
    panel_style::text_right(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(right - region_width, inner.top()),
            egui::pos2(right, inner.bottom()),
        ),
        region(option),
        HINT_SIZE,
        theme::text_muted(ui),
    );
    panel_style::text(
        ui,
        egui::Rect::from_min_max(
            inner.min,
            egui::pos2(right - region_width - 8.0, inner.bottom()),
        ),
        option.label,
        13.0,
        theme::text_primary(ui),
    );
    response.on_hover_text(if current {
        format!("{} · the file's current encoding", option.label)
    } else {
        option.label.to_owned()
    })
}

fn supports_bom(encoding: &str) -> bool {
    FileService::encoding_supports_bom(encoding).unwrap_or(false)
}

fn option_label(choice: &str) -> &str {
    COMMON_TEXT_ENCODINGS
        .iter()
        .find(|option| option.canonical_name.eq_ignore_ascii_case(choice))
        .map_or(choice, |option| option.label)
}

fn footer(ui: &mut egui::Ui, app: &mut ScratchpadApp, facts: &FileFacts, actions: &mut Actions) {
    let dialog = &mut app.state.dialogs.encoding;
    let label = option_label(&dialog.choice).to_owned();
    let saving = dialog.mode == EncodingMode::Save;
    if saving && supports_bom(&dialog.choice) {
        ui.checkbox(&mut dialog.bom, format!("{label} with BOM"))
            .on_hover_text("Start the file with a byte order mark. Some Windows tools expect one; most other tools do not.");
    }
    let (note, warn) = mode_note(dialog.mode, &dialog.choice, &label, facts);
    let color = if warn {
        ui.visuals().warn_fg_color
    } else {
        theme::text_muted(ui)
    };
    ui.add(egui::Label::new(egui::RichText::new(note).size(NOTE_SIZE).color(color)).wrap());
    let enabled = saving || facts.reopen_blocked().is_none();
    let verb = if saving { "Save" } else { "Reopen" };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(KEY_HINTS).size(HINT_SIZE).weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let button = window::primary_button(ui, format!("{verb} as {label}"), enabled);
            actions.apply |= ui.add_enabled(enabled, button).clicked();
        });
    });
    if !enabled {
        actions.apply = false;
    }
}

/// What the chosen action will do, and whether that needs a warning colour.
fn mode_note(mode: EncodingMode, choice: &str, label: &str, facts: &FileFacts) -> (String, bool) {
    let unicode = choice.to_ascii_uppercase().starts_with("UTF-");
    match (mode, facts.reopen_blocked()) {
        (EncodingMode::Reopen, Some(reason)) => (reason.to_owned(), true),
        (EncodingMode::Reopen, None) => (
            format!(
                "Reads the file from disk again as {label}. Nothing is written until you save."
            ),
            false,
        ),
        (EncodingMode::Save, _) if !unicode => (
            format!(
                "If the text has characters {label} cannot represent, the save stops and the file is unchanged."
            ),
            true,
        ),
        (EncodingMode::Save, _) if facts.path.is_none() => {
            (format!("Choose where to save, as {label}."), false)
        }
        (EncodingMode::Save, _) => (format!("Writes the file as {label}."), false),
    }
}

/// Runs the chosen action; returns true when it started.
fn apply(app: &mut ScratchpadApp, facts: &FileFacts) -> bool {
    let dialog = &app.state.dialogs.encoding;
    let encoding_name = dialog.choice.clone();
    let command = match dialog.mode {
        EncodingMode::Reopen if facts.reopen_blocked().is_none() => {
            FileCommand::ReopenBufferWithEncoding {
                tab_index: facts.tab_index,
                encoding_name,
            }
        }
        EncodingMode::Reopen => return false,
        EncodingMode::Save => FileCommand::SaveFileWithEncoding {
            tab_index: facts.tab_index,
            bom: Some(dialog.bom && supports_bom(&encoding_name)),
            encoding_name,
        },
    };
    crate::app::commands::handle_command(app, AppCommand::File(command))
}
