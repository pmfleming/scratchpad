//! A live, pane-local zoom control with the history scrubber's focus behavior.
mod track;

use super::scrubber_chrome::{self, Step};
use super::text_history::{editor_input_requested, keys_belong_elsewhere};
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
    theme,
    ui::{panel_style, widget_ids},
};
use eframe::egui;
use egui_phosphor::regular as icons;

const KEY_HINTS: &str = "← → Step   Home End Limits   0 Reset   Esc Close";

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
    if keys_belong_elsewhere(ctx, app) {
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
        (egui::Key::Num0, ZoomChange::Reset),
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
    let mut change = None;
    let fill = app.state.app_settings.editor_background_color();
    scrubber_chrome::show_card(ctx, fill, "zoom_scrubber_window", "Zoom scrubber", |ui| {
        if header(ui, factor) {
            change = Some(ZoomChange::Reset);
        }
        change = transport(ui, factor).or(change);
        ui.label(egui::RichText::new(KEY_HINTS).size(11.0).weak());
    });
    if let Some(change) = change {
        zoom::apply(app, view, change);
        ctx.memory_mut(|m| m.request_focus(focus_id()));
        ctx.request_repaint();
    }
    if ctx.memory(|m| m.has_focus(focus_id())) {
        panel_style::lock_navigation_focus(ctx, focus_id());
    }
}

/// Title, the current percentage and reset. Returns true when reset was clicked.
fn header(ui: &mut egui::Ui, factor: f32) -> bool {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Zoom").size(14.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let reset = panel_style::icon_button(
                ui,
                "reset",
                icons::ARROW_COUNTER_CLOCKWISE,
                "Reset zoom (100%)",
                factor != 1.0,
            )
            .clicked();
            // Painted, not a label: the slider already reports the value.
            let percent = format!("{:.0}%", factor * 100.0);
            let width = ui
                .painter()
                .layout_no_wrap(
                    percent.clone(),
                    panel_style::font(ui, 13.0),
                    theme::text_primary(ui),
                )
                .size()
                .x;
            let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(width, 20.0));
            panel_style::text(ui, rect, &percent, 13.0, theme::text_primary(ui));
            reset
        })
        .inner
    })
    .inner
}

fn transport(ui: &mut egui::Ui, factor: f32) -> Option<ZoomChange> {
    let above_min = factor > MIN_ZOOM_FACTOR;
    let below_max = factor < MAX_ZOOM_FACTOR;
    let mut change = None;
    scrubber_chrome::transport(
        ui,
        &mut change,
        |_| {
            [
                Step {
                    key: "minimum",
                    icon: icons::SKIP_BACK,
                    label: "Minimum zoom (Home)",
                    target: ZoomChange::Factor(MIN_ZOOM_FACTOR),
                    enabled: above_min,
                },
                Step {
                    key: "decrease",
                    icon: icons::CARET_LEFT,
                    label: "Zoom out (←)",
                    target: ZoomChange::Out,
                    enabled: above_min,
                },
            ]
        },
        |ui, change, width| {
            let mut percent = factor * 100.0;
            if track::show(ui, &mut percent, width).changed() {
                *change = Some(ZoomChange::Factor(percent / 100.0));
            }
        },
        |_| {
            [
                Step {
                    key: "increase",
                    icon: icons::CARET_RIGHT,
                    label: "Zoom in (→)",
                    target: ZoomChange::In,
                    enabled: below_max,
                },
                Step {
                    key: "maximum",
                    icon: icons::SKIP_FORWARD,
                    label: "Maximum zoom (End)",
                    target: ZoomChange::Factor(MAX_ZOOM_FACTOR),
                    enabled: below_max,
                },
            ]
        },
        |change, target| *change = Some(target),
    );
    change
}

#[cfg(test)]
mod tests;
