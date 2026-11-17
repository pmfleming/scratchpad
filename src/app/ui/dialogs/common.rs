//! The centred confirm panel shared by file prompts, in History's panel style:
//! a title, the file line, optional content, then key hints and the actions.
use crate::app::theme;
use crate::app::ui::{callout, panel_style, settings, widget_ids};
use eframe::egui;

const HINT_SIZE: f32 = 11.0;
const PATH_SIZE: f32 = 12.0;

/// One footer action. `hint` is its tooltip; the panel itself carries no explanation.
pub(super) struct Choice<T> {
    pub(super) label: String,
    pub(super) hint: &'static str,
    pub(super) value: T,
    pub(super) accent: bool,
    pub(super) quiet: bool,
}

impl<T> Choice<T> {
    pub(super) fn new(label: impl Into<String>, hint: &'static str, value: T) -> Self {
        Self {
            label: label.into(),
            hint,
            value,
            accent: false,
            quiet: false,
        }
    }

    pub(super) fn accent(mut self) -> Self {
        self.accent = true;
        self
    }

    /// A frameless action, for Cancel.
    pub(super) fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }
}

pub(super) struct Confirm<'a, T> {
    pub(super) id: &'static str,
    pub(super) title: &'a str,
    pub(super) path: &'a str,
    pub(super) width: f32,
    pub(super) hints: &'static str,
    /// Left to right, safest first; the accent action goes last.
    pub(super) choices: Vec<Choice<T>>,
    pub(super) enter: Option<T>,
    pub(super) escape: T,
}

/// Shows the panel; returns the chosen action, including Enter and Esc.
pub(super) fn show_confirm<T: Copy>(
    ctx: &egui::Context,
    fill: egui::Color32,
    confirm: Confirm<'_, T>,
    header: impl FnOnce(&mut egui::Ui),
    body: impl FnOnce(&mut egui::Ui),
) -> Option<T> {
    let frame = panel_style::window_frame(ctx, fill);
    let layer = widget_ids::area_layer_id(egui::Order::Foreground, confirm.id);
    let mut chosen = None;
    let response = egui::Window::new(confirm.title)
        .id(layer.id)
        .order(layer.order)
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .frame(frame)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .show(ctx, |ui| {
            widget_ids::feature_scope(ui, confirm.id, |ui| {
                ui.set_width(confirm.width);
                settings::apply_dialog_typography(ui);
                callout::apply_spacing(ui);
                panel_style::apply(ui);
                ui.visuals_mut().panel_fill = frame.fill;
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(confirm.title)
                            .size(14.0)
                            .strong()
                            .color(callout::text(ui)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), header);
                });
                path_line(ui, confirm.path);
                body(ui);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(confirm.hints).size(HINT_SIZE).weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for choice in confirm.choices.iter().rev() {
                            if action_button(ui, choice).clicked() {
                                chosen = Some(choice.value);
                            }
                        }
                    });
                });
            });
        });
    if let Some(response) = response {
        callout::mark_scroll_blocker_if_hovered(ctx, &response.response);
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        chosen = Some(confirm.escape);
    }
    if let Some(enter) = confirm.enter
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
    {
        chosen = Some(enter);
    }
    chosen
}

fn path_line(ui: &mut egui::Ui, path: &str) {
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), 18.0));
    panel_style::text_end(ui, rect, path, PATH_SIZE, theme::text_muted(ui));
    let response = widget_ids::interact(
        ui,
        rect,
        egui::Id::unique(("confirm.path", path)),
        egui::Sense::hover(),
        "confirm-path",
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, path));
    response.on_hover_text(path);
}

fn action_button<T>(ui: &mut egui::Ui, choice: &Choice<T>) -> egui::Response {
    let text = egui::RichText::new(&choice.label);
    let button = if choice.accent {
        egui::Button::new(text.color(panel_style::on_accent(ui)))
            .fill(theme::tab_selected_accent(ui))
    } else if choice.quiet {
        egui::Button::new(text).frame(false)
    } else {
        egui::Button::new(text)
    };
    ui.add(button).on_hover_text(choice.hint)
}
