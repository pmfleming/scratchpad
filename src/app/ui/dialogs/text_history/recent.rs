use super::{HistoryUi, presentation, style};
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
    pub token: Option<String>,
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
        token: Option<String>,
        edited: (u64, u64),
        position: Option<(usize, usize)>,
    ) -> Self {
        let path = std::path::Path::new(&label);
        let name = path
            .file_name()
            .map_or_else(|| label.clone(), |name| name.to_string_lossy().into_owned());
        let folder = path
            .parent()
            .map_or_else(String::new, |path| path.display().to_string());
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
                        Some(file.token.clone()),
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
    if response.hovered() || selected {
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
            "Closed".into()
        }
    } else {
        presentation::age(file.edited.0)
    };
    let right = rect.right() - 8.0;
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
    style::text(
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

pub(super) fn picker(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    state: &mut HistoryUi,
) -> Option<String> {
    let buffer = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| state.selected.and_then(|id| tab.buffer_by_id(id)));
    let caption = buffer.map_or_else(
        || "Choose a file".into(),
        |b| {
            format!(
                "{}  ·  {} of {}",
                b.name,
                b.document().operation_undo_depth(),
                b.document().history_route().len()
            )
        },
    );
    let mut caption = style::icon_label(ui, icons::FILE_TEXT, &caption, 13.0);
    caption.append(
        icons::CARET_UP_DOWN,
        10.0,
        egui::TextFormat::simple(egui_phosphor::font_id(13.0), theme::text_muted(ui)),
    );
    let button = ui.add_sized(
        [ui.available_width(), 34.0],
        egui::Button::new(caption)
            .fill(theme::action_bg(ui))
            .truncate(),
    );
    button
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Choose history file"));
    let button = button
        .on_hover_text(buffer.map_or_else(String::new, crate::app::text_history::buffer_label));
    let mut reopen = None;
    egui::Popup::menu(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(button.rect.width())
        .show(|ui| {
            reopen = picker_contents(ui, app, state, button.clicked());
        });
    reopen
}

fn picker_contents(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    state: &mut HistoryUi,
    opening: bool,
) -> Option<String> {
    style::apply(ui);
    let search = search(ui, state);
    if opening {
        search.request_focus();
    }
    if search.changed() {
        state.picker_index = 0;
    }
    let (files, error) = collect(ui, app, &state.search, false);
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
            let selected = if search.has_focus() {
                i == state.picker_index
            } else {
                state.selected == file.id && file.id.is_some()
            };
            if row(ui, file, selected, None).selected {
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
