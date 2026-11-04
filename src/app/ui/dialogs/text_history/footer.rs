use super::{HistoryUi, recent, style};
use crate::app::{app_state::ScratchpadApp, domain::BufferState, theme, ui::widget_ids};
use eframe::egui;

pub(super) fn panel(ui: &mut egui::Ui, app: &ScratchpadApp, state: &mut HistoryUi) -> Option<u64> {
    let key = egui::Id::unique("history.footer.height");
    let height = ui
        .ctx()
        .data(|d| d.get_temp::<f32>(key))
        .unwrap_or(ui.spacing().interact_size.y);
    let result = egui::Panel::bottom(egui::Id::unique("history.footer"))
        .resizable(false)
        .show_separator_line(false)
        .exact_size(height)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let top = ui.cursor().top();
            let action = show(ui, app, state);
            (action, ui.cursor().top() - top)
        })
        .inner;
    // Measure real wrapped content. A discarded sizing pass makes the footer
    // fit before it is painted, preserving the user's window size in both states.
    if (result.1 - height).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(key, result.1));
        ui.ctx()
            .request_discard("History footer content changed size");
    }
    result.0
}

pub(super) fn visible(buffer: &BufferState, state: &HistoryUi) -> bool {
    recent::has_history(buffer)
        && if state.all_files {
            state.expanded.contains(&buffer.id)
                && crate::app::text_history::buffer_label(buffer)
                    .to_lowercase()
                    .contains(&state.search.to_lowercase())
        } else {
            state.selected == Some(buffer.id)
        }
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
        ui.add(
            egui::Label::new(
                egui::RichText::new(format!("Forget history for {}?", buffer.name)).strong(),
            )
            .truncate(),
        )
        .on_hover_text(crate::app::text_history::buffer_label(buffer));
        if buffer.path.is_some() {
            let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), 18.0));
            let path = crate::app::text_history::buffer_label(buffer);
            style::text_end(ui, rect, &path, 11.0, theme::text_muted(ui));
            widget_ids::interact(
                ui,
                rect,
                egui::Id::unique(("history.forget.target", buffer.id)),
                egui::Sense::hover(),
                "history-forget-target",
            )
            .on_hover_text(path);
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
        return forgotten;
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

pub(super) fn file_row(ui: &mut egui::Ui, buffer: &BufferState, state: &mut HistoryUi) {
    let rect =
        widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), style::ROW_HEIGHT));
    let mut child = widget_ids::rect_child_ui(
        ui,
        rect,
        ("history.file.footer", buffer.id),
        egui::Layout::right_to_left(egui::Align::Center),
    );
    forget_button(&mut child, buffer, state);
}
