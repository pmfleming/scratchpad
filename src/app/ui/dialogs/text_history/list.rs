use super::{
    HistoryUi,
    recent::{self, File},
    style,
    tree_list::{self, Prepared},
};
use crate::app::{app_state::ScratchpadApp, domain::BufferState, theme, ui::widget_ids};
use eframe::egui;
use std::ops::Range;

#[derive(Default)]
pub(super) struct Action {
    pub navigate: Option<(u64, u64)>,
    pub reopen: Option<String>,
    pub focus: bool,
}
struct Section<'a> {
    file: Option<&'a File>,
    buffer: Option<&'a BufferState>,
    tree: Option<Prepared>,
    start: usize,
    count: usize,
}

fn sections<'a>(
    ctx: &egui::Context,
    app: &'a ScratchpadApp,
    state: &HistoryUi,
    files: &'a [File],
) -> Vec<Section<'a>> {
    let mut sections = Vec::new();
    let mut total = 0;
    let find = |id| {
        app.tab_manager
            .tabs
            .as_slice()
            .iter()
            .find_map(|t| t.buffer_by_id(id))
    };
    if state.all_files {
        let mut closed_heading = false;
        for file in files {
            if file.token.is_some() && !closed_heading {
                sections.push(Section {
                    file: None,
                    buffer: None,
                    tree: None,
                    start: total,
                    count: 1,
                });
                total += 1;
                closed_heading = true;
            }
            let buffer = file.id.and_then(find);
            let tree = buffer
                .filter(|b| state.expanded.contains(&b.id))
                .map(|b| tree_list::prepare(ctx, b));
            let count = 1 + tree.as_ref().map_or(0, Prepared::len);
            sections.push(Section {
                file: Some(file),
                buffer,
                tree,
                start: total,
                count,
            });
            total += count;
        }
    } else if let Some(buffer) = state.selected.and_then(find) {
        let tree = tree_list::prepare(ctx, buffer);
        let count = tree.len();
        sections.push(Section {
            file: None,
            buffer: Some(buffer),
            tree: Some(tree),
            start: 0,
            count,
        });
    }
    sections
}

fn file_action(ui: &mut egui::Ui, file: &File, state: &mut HistoryUi, action: &mut Action) {
    let expanded = file.id.map(|id| state.expanded.contains(&id));
    let response = recent::row(
        ui,
        file,
        state.selected == file.id && file.id.is_some(),
        expanded,
    );
    if let Some(id) = file.id {
        action.focus |= response.toggled;
        if response.toggled && !state.expanded.remove(&id) {
            state.expanded.insert(id);
        }
        if response.selected {
            state.file_picked = true;
            state.selected = Some(id);
            state.expanded.insert(id);
            action.focus = true;
        }
    } else if response.selected {
        action.reopen.clone_from(&file.token);
    }
}

fn paint_section(
    ui: &mut egui::Ui,
    section: &Section<'_>,
    range: Range<usize>,
    state: &mut HistoryUi,
    action: &mut Action,
) {
    let visible = range.start.max(section.start)..range.end.min(section.start + section.count);
    for index in visible.clone() {
        if let Some(file) = section.file
            && index == section.start
        {
            file_action(ui, file, state, action);
        } else if let (Some(buffer), Some(tree)) = (section.buffer, &section.tree) {
            let offset = section.start + usize::from(section.file.is_some());
            let local_range =
                visible.start.saturating_sub(offset)..visible.end.saturating_sub(offset);
            if tree_list::show_row(ui, buffer, tree, index - offset, local_range) {
                state.selected = Some(buffer.id);
                action.navigate = Some((buffer.id, tree.view_id(index - offset)));
                action.focus = true;
            }
        } else {
            let rect = widget_ids::allocate_exact_rect(
                ui,
                egui::vec2(ui.available_width(), style::ROW_HEIGHT),
            );
            style::text(
                ui,
                rect.shrink2(egui::vec2(8.0, 0.0)),
                "Closed",
                11.5,
                theme::text_muted(ui),
            );
        }
    }
}

fn jump_button(
    ui: &mut egui::Ui,
    output: &egui::scroll_area::ScrollAreaOutput<()>,
    now: usize,
    scroll_key: egui::Id,
    hidden: bool,
) -> bool {
    let top = now as f32 * style::ROW_HEIGHT;
    let viewport_top = output.state.clamped_offset().y;
    let viewport_bottom = viewport_top + output.inner_rect.height();
    let above = top + style::ROW_HEIGHT <= viewport_top;
    let below = top >= viewport_bottom;
    if !above && !below && !hidden {
        return false;
    }
    let label = style::icon_label(
        ui,
        if above {
            egui_phosphor::regular::ARROW_UP
        } else {
            egui_phosphor::regular::ARROW_DOWN
        },
        "Now",
        12.0,
    );
    let rect = egui::Rect::from_center_size(
        egui::pos2(
            output.inner_rect.center().x,
            if above {
                output.inner_rect.top() + 20.0
            } else {
                output.inner_rect.bottom() - 20.0
            },
        ),
        egui::vec2(74.0, 26.0),
    );
    let button = widget_ids::scope(ui, ("history.jump", scroll_key), |ui| {
        ui.put(
            rect,
            egui::Button::new(label).fill(theme::tab_selected_accent(ui)),
        )
    })
    .inner;
    button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Scroll to Now"));
    if button
        .on_hover_text("Scroll to Now without changing the document")
        .clicked()
    {
        ui.ctx()
            .data_mut(|d| d.insert_temp(scroll_key.with("jump"), true));
        ui.ctx().request_repaint();
        return true;
    }
    false
}

pub(super) fn show(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    state: &mut HistoryUi,
    height: f32,
) -> Action {
    let mut action = Action::default();
    let (files, error) = if state.all_files {
        recent::collect(ui, app, &state.search, true)
    } else {
        (Vec::new(), None)
    };
    let banner_height = error.map_or(0.0, |error| {
        ui.colored_label(ui.visuals().error_fg_color, error)
            .rect
            .height()
            + ui.spacing().item_spacing.y
    });
    let height = (height - banner_height).max(style::ROW_HEIGHT);
    let sections = sections(ui.ctx(), app, state, &files);
    let total = sections.last().map_or(0, |s| s.start + s.count);
    let mut now = None;
    let mut changed = false;
    let mut hidden = false;
    for section in &sections {
        if let (Some(buffer), Some(tree)) = (section.buffer, &section.tree)
            && state.selected == Some(buffer.id)
        {
            now = tree
                .now_index(buffer)
                .map(|i| section.start + usize::from(section.file.is_some()) + i);
            changed = tree.changed;
            hidden = tree.now_hidden(buffer);
        }
    }
    let scroll_key = egui::Id::unique(("history.list", state.all_files));
    let selected_key = scroll_key.with("selected");
    let selection_changed =
        ui.ctx().data(|d| d.get_temp::<Option<u64>>(selected_key)) != Some(state.selected);
    let jump = ui
        .ctx()
        .data_mut(|d| d.remove_temp::<bool>(scroll_key.with("jump")))
        .unwrap_or(false);
    ui.ctx()
        .data_mut(|d| d.insert_temp(selected_key, state.selected));
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt(scroll_key)
        .max_height(height)
        .auto_shrink([false, false]);
    if (changed || selection_changed || jump)
        && let Some(now) = now
    {
        scroll = scroll.vertical_scroll_offset(
            (now as f32 * style::ROW_HEIGHT - height * 0.5 + 15.0).max(0.0),
        );
    }
    let output = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            scroll.show_rows(ui, style::ROW_HEIGHT, total.max(1), |ui, range| {
                if total == 0 {
                    ui.add_sized(
                        [ui.available_width(), style::ROW_HEIGHT],
                        egui::Label::new(if state.search.is_empty() {
                            "No files with recorded changes"
                        } else {
                            "No history files match your search"
                        }),
                    );
                    return;
                }
                let first = sections.partition_point(|s| s.start + s.count <= range.start);
                for section in sections[first..].iter().take_while(|s| s.start < range.end) {
                    paint_section(ui, section, range.clone(), state, &mut action);
                }
            })
        })
        .inner;
    if let Some(now) = now
        && jump_button(ui, &output, now, scroll_key, hidden)
    {
        action.focus = true;
        if let Some(buffer) = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .find_map(|t| state.selected.and_then(|id| t.buffer_by_id(id)))
        {
            tree_list::reveal_now(ui.ctx(), buffer);
        }
    }
    action
}
