use super::super::{presentation, tree_list};
use crate::app::{
    domain::BufferState,
    ui::dialogs::scrubber_chrome::{self, Track},
};
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
    let mut track = Track::new(ui, width, super::focus_id(), "history-track");
    let rail = track.rail;
    let x = |position| x_at(rail, len, position);
    let position_at = |x: f32| {
        ((x - rail.left()) / rail.width())
            .clamp(0.0, 1.0)
            .mul_add(len as f32, 0.0)
            .round() as usize
    };
    track.follow_pointer(position, position_at);
    accessibility(ui, &mut track, position, len);
    track.response.widget_info(|| {
        egui::WidgetInfo::slider(ui.is_enabled(), *position as f64, "History position")
    });
    track.paint_rail(ui, x(*position));
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
    for &i in fork_ticks(ui.ctx(), buffer).iter() {
        track.paint_mark(ui, x(i));
    }
    track.paint_thumb(ui, x(*position));
    if let Some(hovered) = track.hovered(ui, position_at, x) {
        let node = hovered
            .checked_sub(1)
            .map_or(0, |i| document.history_route()[i]);
        let change = presentation::change(ui, buffer, node);
        track
            .response
            .clone()
            .on_hover_text_at_pointer(format!("{hovered} · {}", change.title));
    }
    track.response
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

fn accessibility(ui: &egui::Ui, track: &mut Track, position: &mut usize, len: usize) {
    if !ui.is_enabled() {
        return;
    }
    let requests = scrubber_chrome::slider_requests(ui, track.response.id);
    let stepped = position
        .saturating_add(requests.increments)
        .saturating_sub(requests.decrements)
        .min(len);
    let next = requests.value.map_or(stepped, |value| {
        value.round().clamp(0.0, len as f64) as usize
    });
    track.set_requested(position, next);
    scrubber_chrome::describe_slider(
        ui,
        track.response.id,
        0.0..=len as f64,
        1.0,
        *position as f64,
    );
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
