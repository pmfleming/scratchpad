use crate::app::theme::*;
use crate::app::ui::tab_drag;
use eframe::egui::{self, Rect, Sense, Stroke, Vec2};

struct TabButtonFrame {
    rect: Rect,
    response: egui::Response,
    drag_in_progress: bool,
}

pub fn tab_button(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    show_promote_all: bool,
) -> (egui::Response, Option<egui::Response>, egui::Response, bool) {
    tab_button_with_actions(ui, label, active, show_promote_all, TAB_BUTTON_WIDTH)
}

pub fn tab_button_with_actions(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    show_promote_all: bool,
    width: f32,
) -> (egui::Response, Option<egui::Response>, egui::Response, bool) {
    let frame = allocate_tab_button_frame(ui, width);
    paint_tab_background(
        ui,
        frame.rect,
        &frame.response,
        active,
        frame.drag_in_progress,
    );
    let promote_rect = show_promote_all.then(|| tab_promote_rect(frame.rect));
    let truncated = paint_tab_label(ui, frame.rect, label, show_promote_all);
    let promote_response = promote_rect
        .map(|promote_rect| render_tab_promote_button(ui, promote_rect, frame.drag_in_progress));
    let (_, close_response) = render_tab_close_button(ui, frame.rect, frame.drag_in_progress);

    (frame.response, promote_response, close_response, truncated)
}

pub fn tab_button_sized(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    width: f32,
) -> (egui::Response, egui::Response, bool) {
    let frame = allocate_tab_button_frame(ui, width);
    paint_tab_background(
        ui,
        frame.rect,
        &frame.response,
        active,
        frame.drag_in_progress,
    );
    let truncated = paint_tab_label(ui, frame.rect, label, false);
    let (_, close_response) = render_tab_close_button(ui, frame.rect, frame.drag_in_progress);

    (frame.response, close_response, truncated)
}

pub fn tab_button_sized_with_actions(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    show_promote_all: bool,
    width: f32,
) -> (egui::Response, Option<egui::Response>, egui::Response, bool) {
    tab_button_with_actions(ui, label, active, show_promote_all, width)
}

fn paint_tab_background(
    ui: &egui::Ui,
    rect: Rect,
    response: &egui::Response,
    active: bool,
    drag_in_progress: bool,
) {
    if active {
        ui.painter().rect_filled(rect, 4.0, tab_active_bg(ui));
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, border(ui)),
            egui::StrokeKind::Outside,
        );
    } else if response.hovered() && !drag_in_progress {
        ui.painter().rect_filled(rect, 4.0, tab_hover_bg(ui));
    }
}

fn allocate_tab_button_frame(ui: &mut egui::Ui, width: f32) -> TabButtonFrame {
    let size = Vec2::new(width, TAB_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(rect, ui.id().with("tab_button"), Sense::click_and_drag());

    TabButtonFrame {
        rect,
        response,
        drag_in_progress: tab_drag::has_tab_drag_for_context(ui.ctx()),
    }
}

fn paint_tab_label(ui: &egui::Ui, rect: Rect, label: &str, show_promote_all: bool) -> bool {
    let right_padding = label_right_padding(show_promote_all);
    let text_rect = Rect::from_min_max(
        rect.min + Vec2::new(8.0, 0.0),
        rect.max - Vec2::new(right_padding, 0.0),
    );
    let (visible_label, truncated) = truncate_label(ui, label, text_rect.width().max(0.0));
    ui.painter().text(
        text_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        &visible_label,
        egui::TextStyle::Button.resolve(ui.style()),
        text_primary(ui),
    );
    truncated
}

fn label_right_padding(show_promote_all: bool) -> f32 {
    if show_promote_all { 50.0 } else { 28.0 }
}

fn tab_promote_rect(tab_rect: Rect) -> Rect {
    Rect::from_center_size(
        tab_rect.right_center() - Vec2::new(34.0, 0.0),
        Vec2::new(18.0, 18.0),
    )
}

fn render_tab_promote_button(
    ui: &mut egui::Ui,
    promote_rect: Rect,
    drag_in_progress: bool,
) -> egui::Response {
    let promote_response = ui.interact(promote_rect, ui.id().with("promote_all"), Sense::click());
    if promote_response.hovered() && !drag_in_progress {
        ui.painter()
            .rect_filled(promote_rect, 2.0, action_hover_bg(ui));
    }
    ui.painter().text(
        promote_rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::ARROW_SQUARE_UP,
        egui::FontId::proportional(14.0),
        text_primary(ui),
    );
    promote_response.on_hover_text("Promote each file in this workspace to its own tab")
}

fn render_tab_close_button(
    ui: &mut egui::Ui,
    tab_rect: Rect,
    drag_in_progress: bool,
) -> (Rect, egui::Response) {
    let close_rect = Rect::from_center_size(
        tab_rect.right_center() - Vec2::new(14.0, 0.0),
        Vec2::new(18.0, 18.0),
    );

    let close_response = ui.interact(close_rect, ui.id().with("close"), Sense::click());
    paint_tab_close_button(ui, close_rect, close_response.hovered(), drag_in_progress);

    (close_rect, close_response)
}

fn paint_tab_close_button(ui: &egui::Ui, close_rect: Rect, hovered: bool, drag_in_progress: bool) {
    if hovered && !drag_in_progress {
        ui.painter().rect_filled(close_rect, 2.0, CLOSE_HOVER_BG);
    }

    ui.painter().text(
        close_rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::X,
        egui::FontId::proportional(14.0),
        text_primary(ui),
    );
}

fn truncate_label(ui: &egui::Ui, label: &str, available_width: f32) -> (String, bool) {
    let ellipsis = "...";
    if label_fits(ui, label, available_width) {
        return (label.to_owned(), false);
    }
    if !ellipsis_fits(ui, ellipsis, available_width) {
        return (String::new(), true);
    }

    (
        best_truncated_label(ui, label, ellipsis, available_width).unwrap_or_default(),
        true,
    )
}

fn label_fits(ui: &egui::Ui, label: &str, available_width: f32) -> bool {
    text_width(ui, label) <= available_width
}

fn ellipsis_fits(ui: &egui::Ui, ellipsis: &str, available_width: f32) -> bool {
    text_width(ui, ellipsis) <= available_width
}

fn best_truncated_label(
    ui: &egui::Ui,
    label: &str,
    suffix: &str,
    available_width: f32,
) -> Option<String> {
    let boundaries = char_boundaries(label);
    let mut low = 0;
    let mut high = boundaries.len();
    let mut best = None;

    while low < high {
        let candidate = truncation_candidate(label, suffix, &boundaries, low, high);
        if candidate_fits(ui, &candidate.1, available_width) {
            best = Some(candidate.1);
            low = candidate.0 + 1;
        } else {
            high = candidate.0;
        }
    }

    best.or_else(|| full_label_with_suffix_if_fits(ui, label, suffix, available_width))
}

fn char_boundaries(label: &str) -> Vec<usize> {
    label.char_indices().map(|(index, _)| index).collect()
}

fn truncation_candidate(
    label: &str,
    suffix: &str,
    boundaries: &[usize],
    low: usize,
    high: usize,
) -> (usize, String) {
    let mid = low + (high - low) / 2;
    let mid_pos = boundaries[mid];
    (mid, format!("{}{}", &label[..mid_pos], suffix))
}

fn candidate_fits(ui: &egui::Ui, candidate: &str, available_width: f32) -> bool {
    text_width(ui, candidate) <= available_width
}

fn full_label_with_suffix_if_fits(
    ui: &egui::Ui,
    label: &str,
    suffix: &str,
    available_width: f32,
) -> Option<String> {
    let candidate = format!("{label}{suffix}");
    if candidate_fits(ui, &candidate, available_width) {
        Some(candidate)
    } else {
        None
    }
}

fn text_width(ui: &egui::Ui, text: &str) -> f32 {
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(
                text.to_owned(),
                egui::TextStyle::Button.resolve(ui.style()),
                text_primary(ui),
            )
            .size()
            .x
    })
}
