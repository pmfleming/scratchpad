use super::{HistoryUi, style, tree_list};
use crate::app::{app_state::ScratchpadApp, theme, ui::widget_ids};
use eframe::egui;

#[derive(Default)]
pub(super) struct Action {
    pub navigate: Option<(u64, u64)>,
    pub focus: bool,
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
    let mut label = label;
    for part in &mut label.sections {
        part.format.color = style::on_accent(ui);
    }
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
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| state.selected.and_then(|id| tab.buffer_by_id(id)))
    else {
        ui.label(if state.pending_file.is_some() {
            "Opening document…"
        } else {
            "Choose a file to inspect its branches"
        });
        return action;
    };
    let tree = tree_list::prepare(ui.ctx(), buffer);
    let now = tree.now_index(buffer);
    let scroll_key = egui::Id::unique(("history.list", false));
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
    if (tree.changed || selection_changed || jump)
        && let Some(now) = now
    {
        let previous = ui
            .ctx()
            .data(|d| d.get_temp::<(f32, f32)>(scroll_key.with("viewport")));
        if let Some(offset) = follow_offset(now, height, previous, jump || selection_changed) {
            scroll = scroll.vertical_scroll_offset(offset);
        }
    }
    let output = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            scroll.show_rows(ui, style::ROW_HEIGHT, tree.len(), |ui, range| {
                for index in range.clone() {
                    if tree_list::show_row(ui, buffer, &tree, index, range.clone()) {
                        action.navigate = Some((buffer.id, tree.view_id(index)));
                        action.focus = true;
                    }
                }
            })
        })
        .inner;
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            scroll_key.with("viewport"),
            (output.state.clamped_offset().y, output.inner_rect.height()),
        )
    });
    if let Some(now) = now
        && jump_button(ui, &output, now, scroll_key, tree.now_hidden(buffer))
    {
        action.focus = true;
        tree_list::reveal_now(ui.ctx(), buffer);
    }
    action
}

fn follow_offset(
    now: usize,
    height: f32,
    previous: Option<(f32, f32)>,
    center: bool,
) -> Option<f32> {
    let top = now as f32 * style::ROW_HEIGHT;
    if !center && let Some((offset, visible_height)) = previous {
        if top < offset {
            return Some(top);
        }
        if top + style::ROW_HEIGHT > offset + visible_height {
            return Some((top + style::ROW_HEIGHT - visible_height).max(0.0));
        }
        return None;
    }
    Some((top - height * 0.5 + style::ROW_HEIGHT * 0.5).max(0.0))
}
