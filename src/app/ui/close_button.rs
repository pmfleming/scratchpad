//! One flat close control for captions, tabs, tiles and floating panels.
use super::{transition, widget_ids};
use crate::app::theme;
use eframe::egui;
use std::hash::Hash;

pub(crate) struct CloseButton<'a> {
    label: &'a str,
    hint: &'a str,
    size: egui::Vec2,
}

#[cfg(test)]
mod tests;

impl<'a> CloseButton<'a> {
    pub(crate) fn new(label: &'a str) -> Self {
        Self {
            label,
            hint: label,
            size: egui::Vec2::splat(26.0),
        }
    }

    pub(crate) fn size(mut self, size: egui::Vec2) -> Self {
        self.size = size;
        self
    }

    pub(crate) fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    pub(crate) fn show(self, ui: &mut egui::Ui, key: impl Hash) -> egui::Response {
        let response = widget_ids::allocate_exact_interact(
            ui,
            self.size,
            widget_ids::surface_role(key, widget_ids::WidgetRole::IconButton),
            egui::Sense::click(),
            "close_button",
        );
        self.finish(ui, response)
    }

    pub(crate) fn show_at(
        self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        id: egui::Id,
    ) -> egui::Response {
        let response = widget_ids::interact(ui, rect, id, egui::Sense::click(), "close_button");
        self.finish(ui, response)
    }

    fn finish(self, ui: &egui::Ui, response: egui::Response) -> egui::Response {
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, response.enabled(), self.label)
        });
        let highlighted = response.enabled()
            && response.hovered()
            && !transition::suppress_interactive_chrome(ui.ctx());
        let color = if highlighted {
            ui.painter().rect_filled(
                response.rect,
                egui::CornerRadius::ZERO,
                theme::CLOSE_HOVER_BG,
            );
            egui::Color32::WHITE
        } else {
            theme::text_primary(ui)
        };
        if response.has_focus() {
            ui.painter().rect_stroke(
                response.rect,
                egui::CornerRadius::ZERO,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Inside,
            );
        }
        let icon_size = (response.rect.size().min_elem() * 0.75).min(16.0);
        ui.painter().text(
            response.rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::X,
            egui_phosphor::font_id(icon_size),
            color,
        );
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(self.hint)
    }
}
