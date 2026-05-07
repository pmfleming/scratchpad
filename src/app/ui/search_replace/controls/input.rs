use crate::app::theme::text_muted;
use eframe::egui;

const INPUT_HEIGHT: f32 = 36.0;
const ICON_SIZE: f32 = 20.0;
const SEARCH_INPUT_CORNER_RADIUS: u8 = 8;

pub(super) fn icon_text_input(
    ui: &mut egui::Ui,
    icon: &str,
    text: &mut String,
    id: egui::Id,
    hint: &str,
) -> egui::Response {
    ui.horizontal(|ui| {
        input_leading_icon(ui, icon);
        compact_text_field(ui, text, id, hint, ui.available_width())
    })
    .inner
}

fn input_leading_icon(ui: &mut egui::Ui, icon: &str) {
    ui.allocate_ui(egui::vec2(28.0, INPUT_HEIGHT), |ui| {
        ui.with_layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.label(
                    egui::RichText::new(icon)
                        .font(egui::FontId::proportional(ICON_SIZE))
                        .color(text_muted(ui)),
                );
            },
        );
    });
}

fn compact_text_field(
    ui: &mut egui::Ui,
    text: &mut String,
    id: egui::Id,
    hint: &str,
    width: f32,
) -> egui::Response {
    let inner = egui::Frame::NONE
        .fill(ui.visuals().widgets.inactive.weak_bg_fill)
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius::same(SEARCH_INPUT_CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(2, 0))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(width, INPUT_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        search_text_edit(text, id, hint)
                            .frame(egui::Frame::NONE)
                            .desired_width(width),
                    )
                },
            )
            .inner
        });

    ui.painter().rect_stroke(
        inner.response.rect,
        egui::CornerRadius::same(SEARCH_INPUT_CORNER_RADIUS),
        input_border_stroke(ui, &inner.inner),
        egui::StrokeKind::Inside,
    );

    inner.inner
}

fn input_border_stroke(ui: &egui::Ui, response: &egui::Response) -> egui::Stroke {
    if response.has_focus() {
        ui.visuals().widgets.active.bg_stroke
    } else if response.hovered() {
        ui.visuals().widgets.hovered.bg_stroke
    } else {
        ui.visuals().widgets.inactive.bg_stroke
    }
}

fn search_text_edit<'a>(text: &'a mut String, id: egui::Id, hint: &str) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .id(id)
        .hint_text(hint)
        .margin(egui::Margin::symmetric(10, 6))
        .vertical_align(egui::Align::Center)
}
