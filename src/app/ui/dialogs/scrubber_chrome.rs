//! The floating card, transport row and slider track shared by the History and Zoom
//! scrubbers.
use crate::app::{
    theme,
    ui::{callout, panel_style, widget_ids},
};
use eframe::egui;
use std::ops::RangeInclusive;

/// Shows `add_contents` in a movable card docked to the bottom-right of the screen.
pub(super) fn show_card(
    ctx: &egui::Context,
    fill: egui::Color32,
    area_key: &'static str,
    name: &'static str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let screen = ctx.content_rect();
    let width = (screen.width() - 24.0).clamp(160.0, 480.0);
    let frame = panel_style::window_frame(ctx, fill);
    widget_ids::area(area_key)
        .accessible_name(name)
        .role(egui::Role::Dialog)
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(true)
        .default_size(egui::vec2(width, 104.0))
        .default_pos(egui::pos2(
            (screen.right() - width - 24.0).max(screen.left()),
            (screen.bottom() - 150.0).max(screen.top()),
        ))
        .show(ctx, |ui| {
            panel_style::apply(ui);
            let inner = frame.show(ui, |ui| {
                let content_width = (width - 24.0).max(1.0);
                ui.set_width(content_width);
                ui.set_min_width(content_width);
                ui.set_max_width(content_width);
                add_contents(ui);
            });
            callout::mark_scroll_blocker_if_hovered(ctx, &inner.response);
        });
}

/// One icon button beside the track that moves the scrubber to `target`.
pub(super) struct Step<T> {
    pub(super) key: &'static str,
    pub(super) icon: &'static str,
    pub(super) label: &'static str,
    pub(super) target: T,
    pub(super) enabled: bool,
}

/// Two step buttons, the track filling the rest of the row, then two more buttons.
/// Each group of buttons is described from `state` as it is when the group is drawn,
/// and a clicked button's target is applied to `state` straight away.
pub(super) fn transport<S, T, R>(
    ui: &mut egui::Ui,
    state: &mut S,
    before: impl FnOnce(&S) -> [Step<T>; 2],
    track: impl FnOnce(&mut egui::Ui, &mut S, f32) -> R,
    after: impl FnOnce(&S) -> [Step<T>; 2],
    mut apply: impl FnMut(&mut S, T),
) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let track_width = (ui.available_width() - 104.0).max(20.0);
        let mut steps = |ui: &mut egui::Ui, state: &mut S, steps: [Step<T>; 2]| {
            for step in steps {
                if panel_style::icon_button(ui, step.key, step.icon, step.label, step.enabled)
                    .clicked()
                {
                    apply(state, step.target);
                }
            }
        };
        let first = before(state);
        steps(ui, state, first);
        let response = track(ui, state, track_width);
        let last = after(state);
        steps(ui, state, last);
        response
    })
    .inner
}

/// A horizontal slider track: a thin rail with a round thumb.
pub(super) struct Track {
    pub(super) rect: egui::Rect,
    pub(super) rail: egui::Rect,
    pub(super) response: egui::Response,
}

impl Track {
    pub(super) fn new(
        ui: &mut egui::Ui,
        width: f32,
        focus_id: egui::Id,
        debug_name: &'static str,
    ) -> Self {
        let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(width.max(20.0), 28.0));
        let response = widget_ids::interact(
            ui,
            rect,
            focus_id,
            egui::Sense::click_and_drag(),
            debug_name,
        );
        let rail = egui::Rect::from_center_size(
            rect.center(),
            egui::vec2((rect.width() - 14.0).max(1.0), 4.0),
        );
        Self {
            rect,
            rail,
            response,
        }
    }

    /// Moves `value` under the pressed pointer and takes focus.
    pub(super) fn follow_pointer<T: PartialEq>(
        &mut self,
        value: &mut T,
        value_at: impl Fn(f32) -> T,
    ) {
        if (self.response.is_pointer_button_down_on() || self.response.clicked())
            && let Some(pointer) = self.response.interact_pointer_pos()
        {
            let next = value_at(pointer.x);
            if *value != next {
                *value = next;
                self.response.mark_changed();
            }
            self.response.request_focus();
        }
    }

    /// Applies a value an assistive technology asked for.
    pub(super) fn set_requested<T: PartialEq>(&mut self, value: &mut T, next: T) {
        if *value != next {
            *value = next;
            self.response.mark_changed();
            self.response.request_focus();
        }
    }

    /// The rail, filled with the accent colour up to `fill_x`.
    pub(super) fn paint_rail(&self, ui: &egui::Ui, fill_x: f32) {
        ui.painter().rect_filled(self.rail, 0, theme::border(ui));
        ui.painter().rect_filled(
            egui::Rect::from_min_max(self.rail.min, egui::pos2(fill_x, self.rail.bottom())),
            0,
            theme::tab_selected_accent(ui),
        );
    }

    /// A tall muted mark across the rail, such as a fork or 100%.
    pub(super) fn paint_mark(&self, ui: &egui::Ui, x: f32) {
        let y = self.rect.center().y;
        ui.painter().line_segment(
            [egui::pos2(x, y - 6.0), egui::pos2(x, y + 6.0)],
            egui::Stroke::new(2.0, theme::text_muted(ui)),
        );
    }

    /// The thumb at `x`, ringed while the track has focus.
    pub(super) fn paint_thumb(&self, ui: &egui::Ui, x: f32) {
        let center = egui::pos2(x, self.rect.center().y);
        let accent = theme::tab_selected_accent(ui);
        if self.response.has_focus() {
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
    }

    /// The value under a hovering, unpressed pointer, marked with a small dot.
    pub(super) fn hovered<T: Copy>(
        &self,
        ui: &egui::Ui,
        value_at: impl Fn(f32) -> T,
        x: impl Fn(T) -> f32,
    ) -> Option<T> {
        if !self.response.hovered() || self.response.is_pointer_button_down_on() {
            return None;
        }
        let hovered = value_at(self.response.hover_pos()?.x);
        ui.painter().circle_filled(
            egui::pos2(x(hovered), self.rect.center().y),
            3.5,
            theme::text_muted(ui),
        );
        Some(hovered)
    }
}

/// Increment, decrement and set-value requests from assistive technology this frame.
pub(super) struct SliderRequests {
    pub(super) increments: usize,
    pub(super) decrements: usize,
    /// The last finite value asked for.
    pub(super) value: Option<f64>,
}

pub(super) fn slider_requests(ui: &egui::Ui, id: egui::Id) -> SliderRequests {
    use egui::accesskit::{Action, ActionData};
    ui.input(|input| SliderRequests {
        increments: input.num_accesskit_action_requests(id, Action::Increment),
        decrements: input.num_accesskit_action_requests(id, Action::Decrement),
        value: input
            .accesskit_action_requests(id, Action::SetValue)
            .filter_map(|request| match request.data {
                Some(ActionData::NumericValue(value)) if value.is_finite() => Some(value),
                _ => None,
            })
            .last(),
    })
}

/// Describes the slider's range and the actions available at `value`.
pub(super) fn describe_slider(
    ui: &egui::Ui,
    id: egui::Id,
    range: RangeInclusive<f64>,
    step: f64,
    value: f64,
) {
    use egui::accesskit::Action;
    ui.ctx().accesskit_node_builder(id, |node| {
        node.set_min_numeric_value(*range.start());
        node.set_max_numeric_value(*range.end());
        node.set_numeric_value_step(step);
        node.add_action(Action::SetValue);
        if value < *range.end() {
            node.add_action(Action::Increment);
        }
        if value > *range.start() {
            node.add_action(Action::Decrement);
        }
    });
}
