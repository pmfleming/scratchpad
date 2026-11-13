use super::super::scrubber_chrome::{self, Track};
use crate::app::domain::view::{MAX_ZOOM_FACTOR, MIN_ZOOM_FACTOR};
use eframe::egui;

const MIN: f32 = MIN_ZOOM_FACTOR * 100.0;
const MAX: f32 = MAX_ZOOM_FACTOR * 100.0;

pub(super) fn show(ui: &mut egui::Ui, percent: &mut f32, width: f32) -> egui::Response {
    let mut track = Track::new(ui, width, super::focus_id(), "zoom-track");
    let rail = track.rail;
    let x = |value| egui::lerp(rail.x_range(), (value - MIN) / (MAX - MIN));
    let value_at = |x: f32| {
        (MIN + (x - rail.left()) / rail.width() * (MAX - MIN))
            .round()
            .clamp(MIN, MAX)
    };
    track.follow_pointer(percent, value_at);
    accessibility(ui, &mut track, percent);
    track.response.widget_info(|| {
        egui::WidgetInfo::slider(ui.is_enabled(), f64::from(*percent), "Zoom percentage")
    });
    track.paint_rail(ui, x(*percent));
    track.paint_mark(ui, x(100.0));
    track.paint_thumb(ui, x(*percent));
    if let Some(hovered) = track.hovered(ui, value_at, x) {
        track
            .response
            .clone()
            .on_hover_text_at_pointer(format!("{hovered:.0}%"));
    }
    track.response
}

fn accessibility(ui: &egui::Ui, track: &mut Track, percent: &mut f32) {
    let requests = scrubber_chrome::slider_requests(ui, track.response.id);
    let stepped = *percent + (requests.increments as f32 - requests.decrements as f32) * 10.0;
    let next = requests
        .value
        .map_or(stepped, |value| value as f32)
        .clamp(MIN, MAX);
    track.set_requested(percent, next);
    scrubber_chrome::describe_slider(
        ui,
        track.response.id,
        f64::from(MIN)..=f64::from(MAX),
        10.0,
        f64::from(*percent),
    );
}
