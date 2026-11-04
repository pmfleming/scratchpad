use super::{HistoryUi, presentation, selection::ArchivedFile, style};
use crate::app::{
    app_state::ScratchpadApp,
    domain::BufferState,
    theme,
    ui::{recovery, widget_ids},
};
use eframe::egui;
use egui_phosphor::regular as icons;

pub(super) struct File {
    pub id: Option<u64>,
    pub token: Option<ArchivedFile>,
    pub label: String,
    pub name: String,
    pub folder: String,
    pub edited: (u64, u64),
    pub position: Option<(usize, usize)>,
}
impl File {
    fn new(
        label: String,
        id: Option<u64>,
        token: Option<ArchivedFile>,
        edited: (u64, u64),
        position: Option<(usize, usize)>,
    ) -> Self {
        let path = std::path::Path::new(&label);
        let name = path
            .file_name()
            .map_or_else(|| label.clone(), |name| name.to_string_lossy().into_owned());
        let folder = path.parent().map_or_else(String::new, short_folder);
        Self {
            id,
            token,
            label,
            name,
            folder,
            edited,
            position,
        }
    }
}

pub(super) fn has_history(buffer: &BufferState) -> bool {
    !buffer.document().history_entries().is_empty()
}

pub(super) fn collect(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    search: &str,
    history_only: bool,
) -> (Vec<File>, Option<String>) {
    let store = app.state.persistence.session_store.clone();
    let key = ("history.recent_files", store.root().to_path_buf());
    let closed = recovery::query(ui, key, move || store.recent_history_files());
    let buffers: Vec<_> = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|t| t.buffers())
        .collect();
    let mut files: Vec<_> = buffers
        .iter()
        .filter(|buffer| !history_only || has_history(buffer))
        .map(|b| {
            File::new(
                crate::app::text_history::buffer_label(b),
                Some(b.id),
                None,
                b.document().last_edit(),
                Some((
                    b.document().operation_undo_depth(),
                    b.document().history_route().len(),
                )),
            )
        })
        .collect();
    let mut error = None;
    if let Some(result) = closed {
        match result {
            Ok(closed) => {
                for file in closed.iter().filter(|file| {
                    (!history_only || file.has_history)
                        && !buffers.iter().any(|buffer| buffer.temp_id == file.temp_id)
                }) {
                    files.push(File::new(
                        file.label.clone(),
                        None,
                        Some(ArchivedFile {
                            token: file.token.clone(),
                            temp_id: file.temp_id.clone(),
                        }),
                        (file.edited_at, file.edit_seq),
                        None,
                    ));
                }
            }
            Err(cause) => error = Some(format!("Could not read recent files: {cause}")),
        }
    }
    let search = search.to_lowercase();
    files.retain(|file| file.label.to_lowercase().contains(&search));
    files.sort_by(|a, b| {
        a.token
            .is_some()
            .cmp(&b.token.is_some())
            .then_with(|| b.edited.cmp(&a.edited))
            .then_with(|| a.label.cmp(&b.label))
    });
    (files, error)
}

pub(super) fn search(ui: &mut egui::Ui, state: &mut HistoryUi) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(&mut state.search)
            .desired_width(f32::INFINITY)
            .hint_text("Find recently edited files")
            .return_key(None)
            .id_salt("history.file_search"),
    )
}

pub(super) struct FileResponse {
    pub selected: bool,
    pub toggled: bool,
}

pub(super) fn row(
    ui: &mut egui::Ui,
    file: &File,
    selected: bool,
    expanded: Option<bool>,
    highlighted: bool,
) -> FileResponse {
    let rect =
        widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), style::ROW_HEIGHT));
    let response = widget_ids::interact(
        ui,
        rect,
        egui::Id::unique(("history.file.row", &file.label, file.id, &file.token)),
        egui::Sense::click(),
        "history-file",
    );
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &file.name));
    if response.hovered() || highlighted || selected {
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
    let mut left = rect.left() + 8.0;
    let mut toggled = false;
    if let Some(expanded) = expanded {
        let toggle_rect = egui::Rect::from_center_size(
            egui::pos2(left + 9.0, rect.center().y),
            egui::vec2(22.0, 24.0),
        );
        let button = widget_ids::scope(ui, ("history.file.toggle", file.id, &file.token), |ui| {
            ui.put(
                toggle_rect,
                egui::Button::new(
                    egui::RichText::new(if expanded {
                        icons::CARET_DOWN
                    } else {
                        icons::CARET_RIGHT
                    })
                    .font(egui_phosphor::font_id(12.0)),
                )
                .frame(false),
            )
        })
        .inner;
        let label = format!(
            "{} history for {}",
            if expanded { "Collapse" } else { "Expand" },
            file.name
        );
        button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        toggled = button.on_hover_text(label).clicked();
        left += 24.0;
    }
    style::text(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(left, rect.top()),
            egui::vec2(20.0, rect.height()),
        ),
        if file.token.is_some() {
            icons::FILE
        } else {
            icons::FILE_TEXT
        },
        14.0,
        theme::text_muted(ui),
    );
    left += 22.0;
    paint_labels(ui, file, rect, left, response.hovered(), selected);
    FileResponse {
        selected: response.on_hover_text(&file.label).clicked() && !toggled,
        toggled,
    }
}

fn paint_labels(
    ui: &egui::Ui,
    file: &File,
    rect: egui::Rect,
    left: f32,
    hovered: bool,
    selected: bool,
) {
    let age = if file.token.is_some() {
        if hovered {
            "Reopen".into()
        } else {
            presentation::age(file.edited.0)
        }
    } else {
        presentation::age(file.edited.0)
    };
    let mut right = rect.right() - 8.0;
    if selected {
        style::text(
            ui,
            egui::Rect::from_min_max(
                egui::pos2(right - 16.0, rect.top()),
                egui::pos2(right, rect.bottom()),
            ),
            icons::CHECK,
            13.0,
            theme::tab_selected_accent(ui),
        );
        right -= 22.0;
    }
    let metadata_width = if let Some((position, count)) = file.position {
        style::text(
            ui,
            egui::Rect::from_min_max(
                egui::pos2(right - 126.0, rect.top()),
                egui::pos2(right - 64.0, rect.bottom()),
            ),
            &format!("{position} of {count}"),
            11.0,
            theme::text_muted(ui),
        );
        132.0
    } else {
        66.0
    };
    style::text(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(right - 60.0, rect.top()),
            egui::pos2(right, rect.bottom()),
        ),
        &age,
        11.0,
        theme::text_muted(ui),
    );
    let available = (right - metadata_width - left).max(0.0);
    let name_width = if file.folder.is_empty() {
        available
    } else {
        available * 0.65
    };
    style::text(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(left, rect.top()),
            egui::vec2(name_width, rect.height()),
        ),
        &file.name,
        13.0,
        if selected {
            theme::tab_selected_accent(ui)
        } else if file.token.is_some() {
            theme::text_muted(ui)
        } else {
            theme::text_primary(ui)
        },
    );
    style::text_end(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(left + name_width + 6.0, rect.top()),
            egui::pos2(right - metadata_width, rect.bottom()),
        ),
        &file.folder,
        11.0,
        theme::text_muted(ui),
    );
    if file.edited.0 != 0 {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_mins(1));
    }
}

fn short_folder(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home
        && let Ok(relative) = path.strip_prefix(std::path::Path::new(&home))
    {
        return if relative.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~{}{}", std::path::MAIN_SEPARATOR, relative.display())
        };
    }
    path.display().to_string()
}

fn selector(ui: &mut egui::Ui, buffer: Option<&BufferState>) -> egui::Response {
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), 34.0));
    let response = widget_ids::interact(
        ui,
        rect,
        egui::Id::unique("history.file.selector"),
        egui::Sense::click(),
        "history-file-selector",
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Choose history file"));
    ui.painter().rect_filled(
        rect,
        0,
        if response.hovered() {
            theme::action_hover_bg(ui)
        } else {
            theme::action_bg(ui)
        },
    );
    let content = rect.shrink2(egui::vec2(10.0, 0.0));
    style::text(ui, content, icons::FILE_TEXT, 14.0, theme::text_muted(ui));
    style::text(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(content.right() - 14.0, content.top()),
            content.max,
        ),
        icons::CARET_UP_DOWN,
        13.0,
        theme::text_muted(ui),
    );
    let count = buffer.map_or_else(String::new, |b| {
        format!(
            "{} of {}",
            b.document().operation_undo_depth(),
            b.document().history_route().len()
        )
    });
    let count_width = ui
        .painter()
        .layout_no_wrap(count.clone(), style::font(ui, 12.0), theme::text_muted(ui))
        .size()
        .x
        .min(content.width() * 0.4);
    let right = content.right() - 24.0;
    style::text_right(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(right - count_width, rect.top()),
            egui::pos2(right, rect.bottom()),
        ),
        &count,
        12.0,
        theme::text_muted(ui),
    );
    let name = buffer.map_or("Choose a file", |b| b.name.as_str());
    let available = (right - count_width - content.left() - 34.0).max(0.0);
    let folder = buffer
        .and_then(|b| b.path.as_ref())
        .and_then(|p| p.parent())
        .map_or_else(String::new, short_folder);
    let name_width = if folder.is_empty() {
        available
    } else {
        (ui.painter()
            .layout_no_wrap(name.into(), style::font(ui, 13.0), theme::text_primary(ui))
            .size()
            .x
            + 4.0)
            .min(available * 0.72)
    };
    let left = content.left() + 24.0;
    style::text(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(left, rect.top()),
            egui::vec2(name_width, rect.height()),
        ),
        name,
        13.0,
        theme::text_primary(ui),
    );
    style::text_end(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(left + name_width + 6.0, rect.top()),
            egui::pos2(right - count_width - 10.0, rect.bottom()),
        ),
        &folder,
        11.0,
        theme::text_muted(ui),
    );
    response
}

pub(super) fn picker(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    state: &mut HistoryUi,
) -> Option<ArchivedFile> {
    let buffer = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| state.selected.and_then(|id| tab.buffer_by_id(id)));
    let button = selector(ui, buffer);
    let button = button
        .on_hover_text(buffer.map_or_else(String::new, crate::app::text_history::buffer_label));
    let was_open = egui::Popup::is_id_open(ui.ctx(), picker_id());
    if was_open && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        egui::Popup::close_id(ui.ctx(), picker_id());
        reset_picker(ui.ctx(), state);
        return None;
    }
    let mut reopen = None;
    egui::Popup::menu(&button)
        .id(picker_id())
        .frame(egui::Frame::popup(ui.style()).corner_radius(egui::CornerRadius::ZERO))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(button.rect.width())
        .show(|ui| {
            reopen = picker_contents(ui, app, state, button.clicked());
        });
    if was_open && !egui::Popup::is_id_open(ui.ctx(), picker_id()) {
        reset_picker(ui.ctx(), state);
    }
    reopen
}

fn picker_contents(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    state: &mut HistoryUi,
    opening: bool,
) -> Option<ArchivedFile> {
    style::apply(ui);
    let search = search(ui, state);
    if opening {
        search.request_focus();
    }
    if search.changed() {
        state.picker_index = 0;
    }
    let (files, error) = collect(ui, app, &state.search, false);
    if opening {
        state.picker_index = files
            .iter()
            .position(|file| file.id.is_some() && file.id == state.selected)
            .unwrap_or(0);
    }
    if let Some(error) = error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    if files.is_empty() {
        ui.weak("No recent files match your search");
    }
    state.picker_index = state.picker_index.min(files.len().saturating_sub(1));
    let mut pick = None;
    let mut moved = false;
    if search.has_focus() {
        ui.input_mut(|input| {
            if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                state.picker_index = (state.picker_index + 1).min(files.len().saturating_sub(1));
                moved = true;
            }
            if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                state.picker_index = state.picker_index.saturating_sub(1);
                moved = true;
            }
            if input.consume_key(egui::Modifiers::NONE, egui::Key::Enter) && !files.is_empty() {
                pick = Some(state.picker_index);
            }
        });
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    let mut scroll = egui::ScrollArea::vertical().max_height(288.0);
    if moved {
        scroll = scroll.vertical_scroll_offset(
            (state.picker_index as f32 * style::ROW_HEIGHT - 140.0).max(0.0),
        );
    }
    scroll.show_rows(ui, style::ROW_HEIGHT, files.len(), |ui, range| {
        for i in range {
            let file = &files[i];
            let selected = state.selected == file.id && file.id.is_some();
            if row(
                ui,
                file,
                selected,
                None,
                search.has_focus() && i == state.picker_index,
            )
            .selected
            {
                pick = Some(i);
            }
        }
    });
    let mut reopen = None;
    if let Some(file) = pick.and_then(|i| files.get(i)) {
        if let Some(id) = file.id {
            state.selected = Some(id);
            state.file_picked = true;
        } else {
            reopen.clone_from(&file.token);
        }
        state.search.clear();
        ui.close();
        ui.ctx().memory_mut(|m| m.request_focus(super::focus_id()));
    }
    reopen
}

pub(super) fn picker_id() -> egui::Id {
    egui::Id::unique("history.file.picker")
}

fn reset_picker(ctx: &egui::Context, state: &mut HistoryUi) {
    state.restore_focus = true;
    state.search.clear();
    state.picker_index = 0;
    ctx.memory_mut(|memory| memory.request_focus(super::focus_id()));
}
