use super::{HistoryUi, recent, style};
use crate::app::{app_state::ScratchpadApp, domain::BufferState, theme, ui::widget_ids};
use eframe::egui;

pub(super) fn panel(ui: &mut egui::Ui, app: &ScratchpadApp, state: &mut HistoryUi) -> Option<u64> {
    let initial_height = ui.spacing().interact_size.y;
    style::window::measured_footer(
        ui,
        "history.footer",
        initial_height,
        "History footer content changed size",
        |ui| show(ui, app, state),
    )
}

pub(super) fn visible(buffer: &BufferState, state: &HistoryUi) -> bool {
    !state.all_files
        && state.pending_file.is_none()
        && state.selected == Some(buffer.id)
        && recent::has_history(buffer)
}

pub(super) fn validate(app: &ScratchpadApp, state: &mut HistoryUi) {
    if let Some(id) = state.forget
        && !app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(|tab| tab.buffers())
            .any(|buffer| buffer.id == id && visible(buffer, state))
    {
        state.forget = None;
    }
}

pub(super) fn forget_button(ui: &mut egui::Ui, buffer: &BufferState, state: &mut HistoryUi) {
    let label = crate::app::text_history::buffer_label(buffer);
    let muted = theme::text_muted(ui);
    let danger = theme::diff_del(ui);
    let button = widget_ids::scope(ui, ("history.forget", buffer.id), |ui| {
        ui.visuals_mut().widgets.inactive.fg_stroke.color = muted;
        ui.visuals_mut().widgets.hovered.fg_stroke.color = danger;
        ui.visuals_mut().widgets.hovered.weak_bg_fill = danger.gamma_multiply(0.12);
        ui.add(egui::Button::new(egui::RichText::new("Forget history…").size(12.0)).frame(false))
    })
    .inner;
    button.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("Forget history for {label}"),
        )
    });
    if button
        .on_hover_text(format!("Forget history for {label}"))
        .clicked()
    {
        state.forget = Some(buffer.id);
    }
}

pub(super) fn show(ui: &mut egui::Ui, app: &ScratchpadApp, state: &mut HistoryUi) -> Option<u64> {
    ui.separator();
    let buffer = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
        .find(|buffer| Some(buffer.id) == state.forget.or(state.selected));
    if let Some(buffer) = buffer.filter(|buffer| state.forget == Some(buffer.id)) {
        return forget_confirmation(ui, buffer, state);
    }
    ui.horizontal(|ui| {
        if ui.available_width() > 380.0 || state.all_files {
            ui.label(
                egui::RichText::new("↑ ↓ Move   Home End Jump   Esc Close")
                    .size(11.0)
                    .weak(),
            );
        }
        if !state.all_files {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(buffer) = buffer.filter(|buffer| visible(buffer, state)) {
                    forget_button(ui, buffer, state);
                }
            });
        }
    });
    None
}

/// Replaces the footer until the user forgets the file's history or cancels.
/// Returns the buffer whose history should be forgotten.
fn forget_confirmation(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    state: &mut HistoryUi,
) -> Option<u64> {
    let label = crate::app::text_history::buffer_label(buffer);
    ui.add(
        egui::Label::new(
            egui::RichText::new(format!("Forget history for {}?", buffer.name)).strong(),
        )
        .truncate(),
    )
    .on_hover_text(&label);
    if buffer.path.is_some() {
        let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), 18.0));
        style::text_end(ui, rect, &label, 11.0, theme::text_muted(ui));
        widget_ids::interact(
            ui,
            rect,
            egui::Id::unique(("history.forget.target", buffer.id)),
            egui::Sense::hover(),
            "history-forget-target",
        )
        .on_hover_text(label);
    }
    ui.label("Forget every branch for this file? Current text is kept. This cannot be undone.");
    let mut forgotten = None;
    ui.horizontal_wrapped(|ui| {
        if ui.button("Cancel").clicked() {
            state.forget = None;
        }
        let button = egui::Button::new(
            egui::RichText::new("Forget this file's history").color(egui::Color32::WHITE),
        )
        .fill(theme::CLOSE_HOVER_BG);
        if ui.add(button).clicked() {
            forgotten = Some(buffer.id);
            state.forget = None;
        }
    });
    forgotten
}
