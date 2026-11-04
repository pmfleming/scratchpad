use super::super::{presentation, tree_list};
use crate::app::{domain::BufferState, theme, ui::widget_ids};
use eframe::egui;
use std::sync::Arc;

#[derive(Clone)]
struct ForkTicks {
    structure: (u64, usize, u64),
    tip: u64,
    ticks: Arc<[usize]>,
}

pub(super) fn show(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    position: &mut usize,
    width: f32,
) -> egui::Response {
    let document = buffer.document();
    let len = document.history_route().len();
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(width.max(20.0), 28.0));
    let mut response = widget_ids::interact(
        ui,
        rect,
        super::focus_id(),
        egui::Sense::click_and_drag(),
        "history-track",
    );
    let rail = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2((rect.width() - 14.0).max(1.0), 4.0),
    );
    let x = |position| x_at(rail, len, position);
    let position_at = |x: f32| {
        ((x - rail.left()) / rail.width())
            .clamp(0.0, 1.0)
            .mul_add(len as f32, 0.0)
            .round() as usize
    };
    if (response.is_pointer_button_down_on() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let target = position_at(pointer.x);
        if *position != target {
            *position = target;
            response.mark_changed();
        }
        response.request_focus();
    }
    accessibility(ui, &mut response, position, len);
    response.widget_info(|| {
        egui::WidgetInfo::slider(ui.is_enabled(), *position as f64, "History position")
    });
    ui.painter().rect_filled(rail, 0, theme::border(ui));
    ui.painter().rect_filled(
        egui::Rect::from_min_max(rail.min, egui::pos2(x(*position), rail.bottom())),
        0,
        theme::tab_selected_accent(ui),
    );
    if len > 0 && len <= 60 {
        for i in 0..=len {
            ui.painter().line_segment(
                [
                    egui::pos2(x(i), rail.top() + 1.0),
                    egui::pos2(x(i), rail.bottom() - 1.0),
                ],
                egui::Stroke::new(1.0, ui.visuals().window_fill),
            );
        }
    }
    let ticks = fork_ticks(ui.ctx(), buffer);
    for &i in ticks.iter() {
        ui.painter().line_segment(
            [
                egui::pos2(x(i), rect.center().y - 6.0),
                egui::pos2(x(i), rect.center().y + 6.0),
            ],
            egui::Stroke::new(2.0, theme::text_muted(ui)),
        );
    }
    let center = egui::pos2(x(*position), rect.center().y);
    let accent = theme::tab_selected_accent(ui);
    if response.has_focus() {
        ui.painter().circle_stroke(
            center,
            11.0,
            egui::Stroke::new(2.0, accent.gamma_multiply(0.45)),
        );
    }
    ui.painter().circle(
        center,
        6.0,
        theme::text_primary(ui),
        egui::Stroke::new(3.0, accent),
    );
    if response.hovered()
        && !response.is_pointer_button_down_on()
        && let Some(pointer) = response.hover_pos()
    {
        let hovered = position_at(pointer.x);
        ui.painter().circle_filled(
            egui::pos2(x(hovered), rect.center().y),
            3.5,
            theme::text_muted(ui),
        );
        let node = hovered
            .checked_sub(1)
            .map_or(0, |i| document.history_route()[i]);
        let change = presentation::change(ui, buffer, node);
        response
            .clone()
            .on_hover_text_at_pointer(format!("{hovered} · {}", change.title));
    }
    response
}

fn x_at(rail: egui::Rect, len: usize, position: usize) -> f32 {
    egui::lerp(
        rail.x_range(),
        if len == 0 {
            0.0
        } else {
            position as f32 / len as f32
        },
    )
}

fn accessibility(ui: &egui::Ui, response: &mut egui::Response, position: &mut usize, len: usize) {
    use egui::accesskit::{Action, ActionData};
    if !ui.is_enabled() {
        return;
    }
    let next = ui.input(|input| {
        let increment = input.num_accesskit_action_requests(response.id, Action::Increment);
        let decrement = input.num_accesskit_action_requests(response.id, Action::Decrement);
        let stepped = position
            .saturating_add(increment)
            .saturating_sub(decrement)
            .min(len);
        input
            .accesskit_action_requests(response.id, Action::SetValue)
            .filter_map(|request| match request.data {
                Some(ActionData::NumericValue(value)) if value.is_finite() => {
                    Some(value.round().clamp(0.0, len as f64) as usize)
                }
                _ => None,
            })
            .last()
            .unwrap_or(stepped)
    });
    if *position != next {
        *position = next;
        response.mark_changed();
        response.request_focus();
    }
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(len as f64);
        node.set_numeric_value_step(1.0);
        node.add_action(Action::SetValue);
        if *position < len {
            node.add_action(Action::Increment);
        }
        if *position > 0 {
            node.add_action(Action::Decrement);
        }
    });
}

fn fork_ticks(ctx: &egui::Context, buffer: &BufferState) -> Arc<[usize]> {
    let document = buffer.document();
    let key = egui::Id::unique(("history.track.forks", buffer.id));
    let structure = document.history_structure_key();
    let tip = document.history_tip();
    let cached = ctx.data(|d| d.get_temp::<ForkTicks>(key));
    if let Some(cached) = cached.filter(|cached| cached.structure == structure && cached.tip == tip)
    {
        return cached.ticks;
    }
    let forks = tree_list::forks(ctx, buffer);
    let ticks: Arc<[usize]> = if forks.is_empty() {
        Arc::from([])
    } else {
        std::iter::once(0)
            .chain(document.history_route().iter().copied())
            .enumerate()
            .filter_map(|(i, id)| forks.contains(&id).then_some(i))
            .collect::<Vec<_>>()
            .into()
    };
    ctx.data_mut(|d| {
        d.insert_temp(
            key,
            ForkTicks {
                structure,
                tip,
                ticks: ticks.clone(),
            },
        )
    });
    ticks
}
