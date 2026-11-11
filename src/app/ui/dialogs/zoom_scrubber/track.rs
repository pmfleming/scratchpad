use crate::app::{
    domain::view::{MAX_ZOOM_FACTOR, MIN_ZOOM_FACTOR},
    theme,
    ui::widget_ids,
};
use eframe::egui;

const MIN: f32 = MIN_ZOOM_FACTOR * 100.0;
const MAX: f32 = MAX_ZOOM_FACTOR * 100.0;

pub(super) fn show(ui: &mut egui::Ui, percent: &mut f32, width: f32) -> egui::Response {
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(width, 28.0));
    let mut response = widget_ids::interact(
        ui,
        rect,
        super::focus_id(),
        egui::Sense::click_and_drag(),
        "zoom-track",
    );
    let rail =
        egui::Rect::from_center_size(rect.center(), egui::vec2((width - 14.0).max(1.0), 4.0));
    let x = |value| egui::lerp(rail.x_range(), (value - MIN) / (MAX - MIN));
    let value_at = |x: f32| {
        (MIN + (x - rail.left()) / rail.width() * (MAX - MIN))
            .round()
            .clamp(MIN, MAX)
    };
    if (response.is_pointer_button_down_on() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let next = value_at(pointer.x);
        if *percent != next {
            *percent = next;
            response.mark_changed();
        }
        response.request_focus();
    }
    accessibility(ui, &mut response, percent);
    response.widget_info(|| {
        egui::WidgetInfo::slider(ui.is_enabled(), f64::from(*percent), "Zoom percentage")
    });
    let accent = theme::tab_selected_accent(ui);
    ui.painter().rect_filled(rail, 0, theme::border(ui));
    ui.painter().rect_filled(
        egui::Rect::from_min_max(rail.min, egui::pos2(x(*percent), rail.bottom())),
        0,
        accent,
    );
    ui.painter().line_segment(
        [
            egui::pos2(x(100.0), rect.center().y - 6.0),
            egui::pos2(x(100.0), rect.center().y + 6.0),
        ],
        egui::Stroke::new(2.0, theme::text_muted(ui)),
    );
    let center = egui::pos2(x(*percent), rect.center().y);
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
        let hovered = value_at(pointer.x);
        ui.painter().circle_filled(
            egui::pos2(x(hovered), rect.center().y),
            3.5,
            theme::text_muted(ui),
        );
        response
            .clone()
            .on_hover_text_at_pointer(format!("{hovered:.0}%"));
    }
    response
}

fn accessibility(ui: &egui::Ui, response: &mut egui::Response, percent: &mut f32) {
    use egui::accesskit::{Action, ActionData};
    let next = ui.input(|input| {
        let increment = input.num_accesskit_action_requests(response.id, Action::Increment);
        let decrement = input.num_accesskit_action_requests(response.id, Action::Decrement);
        let stepped = *percent + (increment as f32 - decrement as f32) * 10.0;
        input
            .accesskit_action_requests(response.id, Action::SetValue)
            .filter_map(|request| match request.data {
                Some(ActionData::NumericValue(value)) if value.is_finite() => Some(value as f32),
                _ => None,
            })
            .last()
            .unwrap_or(stepped)
            .clamp(MIN, MAX)
    });
    if *percent != next {
        *percent = next;
        response.mark_changed();
        response.request_focus();
    }
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_min_numeric_value(f64::from(MIN));
        node.set_max_numeric_value(f64::from(MAX));
        node.set_numeric_value_step(10.0);
        node.add_action(Action::SetValue);
        if *percent < MAX {
            node.add_action(Action::Increment);
        }
        if *percent > MIN {
            node.add_action(Action::Decrement);
        }
    });
}
