//! A live, pane-local zoom control with the history scrubber's focus behavior.
mod track;

use super::text_history::{editor_input_requested, style};
use crate::app::{
    app_state::{
        ScratchpadApp, settings_state,
        workspace::{
            accessors,
            zoom::{self, ZoomChange},
        },
    },
    domain::{
        ViewId,
        view::{MAX_ZOOM_FACTOR, MIN_ZOOM_FACTOR},
    },
    ui::{callout, widget_ids},
};
use eframe::egui;
use egui_phosphor::regular as icons;

fn focus_id() -> egui::Id {
    egui::Id::unique("zoom.scrubber.track")
}

fn active_view(app: &ScratchpadApp) -> Option<ViewId> {
    if settings_state::showing_settings(app) {
        return None;
    }
    app.tab_manager
        .active_tab()
        .map(|tab| tab.layout.active_view_id())
}

pub(crate) fn open(app: &mut ScratchpadApp) {
    if let Some(view) = active_view(app) {
        app.state.dialogs.zoom_scrubber = Some(view);
        app.state.dialogs.history_scrubber.close();
    }
}

fn close(ctx: &egui::Context, app: &mut ScratchpadApp) {
    app.state.dialogs.zoom_scrubber = None;
    if ctx.memory(|m| m.has_focus(focus_id())) {
        ctx.memory_mut(|m| m.surrender_focus(focus_id()));
        accessors::request_focus_for_active_view(app);
    }
}

pub(crate) fn handle_input(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(view) = app.state.dialogs.zoom_scrubber else {
        return;
    };
    if active_view(app) != Some(view) {
        close(ctx, app);
        return;
    }
    if app.state.dialogs.any_modal_open()
        || egui::Popup::is_any_open(ctx)
        || accessors::pending_action(app).is_some()
    {
        return;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        || editor_input_requested(ctx)
    {
        close(ctx, app);
        return;
    }
    if !ctx.memory(|m| m.has_focus(focus_id())) {
        return;
    }
    for (key, change) in [
        (egui::Key::ArrowLeft, ZoomChange::Out),
        (egui::Key::ArrowDown, ZoomChange::Out),
        (egui::Key::ArrowRight, ZoomChange::In),
        (egui::Key::ArrowUp, ZoomChange::In),
        (egui::Key::Home, ZoomChange::Factor(MIN_ZOOM_FACTOR)),
        (egui::Key::End, ZoomChange::Factor(MAX_ZOOM_FACTOR)),
    ] {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
            zoom::apply(app, view, change);
        }
    }
}

pub(crate) fn show(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(view) = app.state.dialogs.zoom_scrubber else {
        return;
    };
    // A pane switch during a drag must never apply that drag to the new pane.
    if active_view(app) != Some(view) {
        close(ctx, app);
        return;
    }
    let tab = app.tab_manager.active_tab().unwrap();
    let factor = tab.layout.active_view().unwrap().zoom_factor();
    let name = tab.active_buffer().name.clone();
    let screen = ctx.content_rect();
    let width = (screen.width() - 24.0).clamp(160.0, 480.0);
    let mut change = None;
    widget_ids::area("zoom_scrubber_window")
        .accessible_name("Zoom scrubber")
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
            style::apply(ui);
            let inner = callout::frame(ui)
                .corner_radius(egui::CornerRadius::ZERO)
                .show(ui, |ui| {
                    ui.set_width((width - 24.0).max(1.0));
                    ui.set_min_width((width - 24.0).max(1.0));
                    ui.set_max_width((width - 24.0).max(1.0));
                    ui.horizontal(|ui| {
                        ui.strong(format!("Zoom · {:.0}%", factor * 100.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if style::icon_button(
                                ui,
                                "reset",
                                icons::ARROW_COUNTER_CLOCKWISE,
                                "Reset zoom (100%)",
                                factor != 1.0,
                            )
                            .clicked()
                            {
                                change = Some(ZoomChange::Reset);
                            }
                        });
                    });
                    change = transport(ui, factor).or(change);
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(name).truncate());
                        ui.weak("25–400% · This pane");
                    });
                });
            callout::mark_scroll_blocker_if_hovered(ctx, &inner.response);
        });
    if let Some(change) = change {
        zoom::apply(app, view, change);
        ctx.memory_mut(|m| m.request_focus(focus_id()));
        ctx.request_repaint();
    }
    if ctx.memory(|m| m.has_focus(focus_id())) {
        ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                focus_id(),
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                    tab: false,
                },
            );
        });
    }
}

fn transport(ui: &mut egui::Ui, factor: f32) -> Option<ZoomChange> {
    let mut change = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let track_width = (ui.available_width() - 104.0).max(20.0);
        for (key, icon, label, target, enabled) in [
            (
                "minimum",
                icons::SKIP_BACK,
                "Minimum zoom (Home)",
                ZoomChange::Factor(MIN_ZOOM_FACTOR),
                factor > MIN_ZOOM_FACTOR,
            ),
            (
                "decrease",
                icons::CARET_LEFT,
                "Zoom out (←)",
                ZoomChange::Out,
                factor > MIN_ZOOM_FACTOR,
            ),
        ] {
            if style::icon_button(ui, key, icon, label, enabled).clicked() {
                change = Some(target);
            }
        }
        let mut percent = factor * 100.0;
        if track::show(ui, &mut percent, track_width).changed() {
            change = Some(ZoomChange::Factor(percent / 100.0));
        }
        for (key, icon, label, target, enabled) in [
            (
                "increase",
                icons::CARET_RIGHT,
                "Zoom in (→)",
                ZoomChange::In,
                factor < MAX_ZOOM_FACTOR,
            ),
            (
                "maximum",
                icons::SKIP_FORWARD,
                "Maximum zoom (End)",
                ZoomChange::Factor(MAX_ZOOM_FACTOR),
                factor < MAX_ZOOM_FACTOR,
            ),
        ] {
            if style::icon_button(ui, key, icon, label, enabled).clicked() {
                change = Some(target);
            }
        }
    });
    change
}

#[cfg(test)]
mod tests;
