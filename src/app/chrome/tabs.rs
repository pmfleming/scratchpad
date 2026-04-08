use crate::app::theme::*;
use crate::app::ui::tab_drag;
use eframe::egui::{self, Rect, Sense, Stroke, Vec2};

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
    let size = Vec2::new(width, TAB_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(rect, ui.id().with("tab_button"), Sense::click_and_drag());
    let drag_in_progress = tab_drag::has_tab_drag_for_context(ui.ctx());

    paint_tab_background(ui, rect, &response, active, drag_in_progress);
    let promote_rect = show_promote_all.then(|| tab_promote_rect(rect));
    let truncated = paint_tab_label(ui, rect, label, show_promote_all);
    let promote_response = promote_rect
        .map(|promote_rect| render_tab_promote_button(ui, promote_rect, drag_in_progress));
    let (_, close_response) = render_tab_close_button(ui, rect, drag_in_progress);

    (response, promote_response, close_response, truncated)
}

pub fn tab_button_sized(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    width: f32,
) -> (egui::Response, egui::Response, bool) {
    let size = Vec2::new(width, TAB_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(rect, ui.id().with("tab_button"), Sense::click_and_drag());
    let drag_in_progress = tab_drag::has_tab_drag_for_context(ui.ctx());

    paint_tab_background(ui, rect, &response, active, drag_in_progress);
    let truncated = paint_tab_label(ui, rect, label, false);
    let (_, close_response) = render_tab_close_button(ui, rect, drag_in_progress);

    (response, close_response, truncated)
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
        ui.painter().rect_filled(rect, 4.0, TAB_ACTIVE_BG);
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, BORDER),
            egui::StrokeKind::Outside,
        );
    } else if response.hovered() && !drag_in_progress {
        ui.painter().rect_filled(rect, 4.0, TAB_HOVER_BG);
    }
}

fn paint_tab_label(ui: &egui::Ui, rect: Rect, label: &str, show_promote_all: bool) -> bool {
    let right_padding = if show_promote_all { 50.0 } else { 28.0 };
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
        TEXT_PRIMARY,
    );
    truncated
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
        ui.painter().rect_filled(promote_rect, 2.0, ACTION_HOVER_BG);
    }
    ui.painter().text(
        promote_rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::ARROW_SQUARE_UP,
        egui::FontId::proportional(14.0),
        TEXT_PRIMARY,
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
        TEXT_PRIMARY,
    );
}

fn truncate_label(ui: &egui::Ui, label: &str, available_width: f32) -> (String, bool) {
    if text_width(ui, label) <= available_width {
        return (label.to_owned(), false);
    }

    let ellipsis = "...";
    if text_width(ui, ellipsis) > available_width {
        return (String::new(), true);
    }

    match find_max_prefix(ui, label, ellipsis, available_width) {
        Some(truncated) => (truncated, true),
        None => (String::new(), true),
    }
}

fn find_max_prefix(
    ui: &egui::Ui,
    label: &str,
    suffix: &str,
    available_width: f32,
) -> Option<String> {
    let boundaries: Vec<usize> = label.char_indices().map(|(i, _)| i).collect();
    let mut low = 0;
    let mut high = boundaries.len();
    let mut best = None;

    while low < high {
        let mid = low + (high - low) / 2;
        let mid_pos = boundaries[mid];
        let candidate = format!("{}{}", &label[..mid_pos], suffix);

        if text_width(ui, &candidate) <= available_width {
            best = Some(candidate);
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    if best.is_none() || low == boundaries.len() {
        let candidate = format!("{label}{suffix}");
        if text_width(ui, &candidate) <= available_width {
            return Some(candidate);
        }
    }

    best
}

fn text_width(ui: &egui::Ui, text: &str) -> f32 {
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(
                text.to_owned(),
                egui::TextStyle::Button.resolve(ui.style()),
                TEXT_PRIMARY,
            )
            .size()
            .x
    })
}
