use crate::app::theme::*;
use eframe::egui::{
    self, Color32, CursorIcon, Rect, Sense, Stroke, Vec2, viewport::ResizeDirection,
};

const RESIZE_BORDER: f32 = 6.0;
const RESIZE_CORNER: f32 = 18.0;

pub fn handle_window_resize(ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    if maximized {
        return;
    }

    let screen_rect = ctx.input(|input| input.screen_rect());
    egui::Area::new(egui::Id::new("window_resize_handles"))
        .fixed_pos(screen_rect.min)
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| render_resize_handles(ui, ctx, screen_rect.size()));
}

fn render_resize_handles(ui: &mut egui::Ui, ctx: &egui::Context, size: Vec2) {
    for grip in resize_grips(size) {
        let response = ui
            .interact(grip.rect, ui.id().with(grip.id), Sense::click_and_drag())
            .on_hover_cursor(grip.cursor);

        if response.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(grip.direction));
        }
    }
}

pub fn phosphor_button(
    ui: &mut egui::Ui,
    icon: &str,
    size: Vec2,
    background: Color32,
    hover_background: Color32,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if response.hovered() {
        hover_background
    } else {
        background
    };

    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(16.0),
        TEXT_PRIMARY,
    );

    response.on_hover_text(tooltip)
}

pub fn tab_button(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
) -> (egui::Response, egui::Response, bool) {
    tab_button_sized(ui, label, active, TAB_BUTTON_WIDTH)
}

pub fn tab_button_sized(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    width: f32,
) -> (egui::Response, egui::Response, bool) {
    let size = Vec2::new(width, TAB_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if active {
        ui.painter().rect_filled(rect, 4.0, TAB_ACTIVE_BG);
        ui.painter()
            .rect_stroke(rect, 4.0, Stroke::new(1.0, BORDER));
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 4.0, TAB_HOVER_BG);
    }

    // Label
    let text_rect = Rect::from_min_max(
        rect.min + Vec2::new(8.0, 0.0),
        rect.max - Vec2::new(28.0, 0.0),
    );
    let available_label_width = text_rect.width().max(0.0);
    let (visible_label, truncated) = truncate_label(ui, label, available_label_width);
    ui.painter().text(
        text_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        &visible_label,
        egui::TextStyle::Button.resolve(ui.style()),
        TEXT_PRIMARY,
    );

    // Close button area (inside the tab)
    let (close_rect, close_response) = render_tab_close_button(ui, rect);

    // Paint the close icon
    ui.painter().text(
        close_rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::X,
        egui::FontId::proportional(14.0),
        TEXT_PRIMARY,
    );

    (response, close_response, truncated)
}

fn render_tab_close_button(ui: &mut egui::Ui, tab_rect: Rect) -> (Rect, egui::Response) {
    let close_rect = Rect::from_center_size(
        tab_rect.right_center() - Vec2::new(14.0, 0.0),
        Vec2::new(18.0, 18.0),
    );

    let close_response = ui.interact(close_rect, ui.id().with("close"), Sense::click());

    if close_response.hovered() {
        ui.painter().rect_filled(close_rect, 2.0, CLOSE_HOVER_BG);
    }

    (close_rect, close_response)
}

pub fn caption_controls(ui: &mut egui::Ui, ctx: &egui::Context, width: f32) -> bool {
    let mut close_requested = false;

    ui.allocate_ui_with_layout(
        egui::vec2(width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            close_requested = render_caption_buttons(ui, ctx);
        },
    );

    close_requested
}

fn render_caption_buttons(ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
    ui.spacing_mut().item_spacing.x = CAPTION_BUTTON_SPACING;

    render_minimize_button(ui, ctx);
    render_maximize_restore_button(ui, ctx);
    let close_requested = render_close_button(ui);

    if CAPTION_TRAILING_PADDING > 0.0 {
        ui.add_space(CAPTION_TRAILING_PADDING);
    }

    close_requested
}

fn render_minimize_button(ui: &mut egui::Ui, ctx: &egui::Context) {
    if phosphor_button(
        ui,
        egui_phosphor::regular::MINUS,
        CAPTION_BUTTON_SIZE,
        ACTION_BG,
        ACTION_HOVER_BG,
        "Minimize",
    )
    .clicked()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
}

fn render_maximize_restore_button(ui: &mut egui::Ui, ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    let (icon, tooltip, next_maximized) = if maximized {
        (egui_phosphor::regular::COPY, "Restore", false)
    } else {
        (egui_phosphor::regular::SQUARE, "Maximize", true)
    };

    if phosphor_button(
        ui,
        icon,
        CAPTION_BUTTON_SIZE,
        ACTION_BG,
        ACTION_HOVER_BG,
        tooltip,
    )
    .clicked()
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(next_maximized));
    }
}

fn render_close_button(ui: &mut egui::Ui) -> bool {
    phosphor_button(
        ui,
        egui_phosphor::regular::X,
        CAPTION_BUTTON_SIZE,
        CLOSE_BG,
        CLOSE_HOVER_BG,
        "Close",
    )
    .clicked()
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
    let mut low = 0;
    let mut high = label.len();
    let mut best = None;

    while low <= high {
        let mid = (low + high) / 2;
        let mut mid = mid;
        while mid > 0 && !label.is_char_boundary(mid) {
            mid -= 1;
        }

        let candidate = format!("{}{}", &label[..mid], suffix);
        if text_width(ui, &candidate) <= available_width {
            best = Some(candidate);
            low = mid + 1;
            while low <= high && !label.is_char_boundary(low) {
                low += 1;
            }
        } else {
            if mid == 0 {
                break;
            }
            high = mid - 1;
        }
    }

    best
}

fn text_width(ui: &egui::Ui, text: &str) -> f32 {
    ui.fonts(|fonts| {
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

struct ResizeGrip {
    id: &'static str,
    rect: Rect,
    direction: ResizeDirection,
    cursor: CursorIcon,
}

fn resize_grips(size: Vec2) -> [ResizeGrip; 8] {
    let width = size.x.max(RESIZE_CORNER * 2.0);
    let height = size.y.max(RESIZE_CORNER * 2.0);

    [
        ResizeGrip {
            id: "north-west",
            rect: Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(RESIZE_CORNER, RESIZE_CORNER),
            ),
            direction: ResizeDirection::NorthWest,
            cursor: CursorIcon::ResizeNwSe,
        },
        ResizeGrip {
            id: "north",
            rect: Rect::from_min_max(
                egui::pos2(RESIZE_CORNER, 0.0),
                egui::pos2(width - RESIZE_CORNER, RESIZE_BORDER),
            ),
            direction: ResizeDirection::North,
            cursor: CursorIcon::ResizeVertical,
        },
        ResizeGrip {
            id: "north-east",
            rect: Rect::from_min_max(
                egui::pos2(width - RESIZE_CORNER, 0.0),
                egui::pos2(width, RESIZE_CORNER),
            ),
            direction: ResizeDirection::NorthEast,
            cursor: CursorIcon::ResizeNeSw,
        },
        ResizeGrip {
            id: "east",
            rect: Rect::from_min_max(
                egui::pos2(width - RESIZE_BORDER, RESIZE_CORNER),
                egui::pos2(width, height - RESIZE_CORNER),
            ),
            direction: ResizeDirection::East,
            cursor: CursorIcon::ResizeHorizontal,
        },
        ResizeGrip {
            id: "south-east",
            rect: Rect::from_min_max(
                egui::pos2(width - RESIZE_CORNER, height - RESIZE_CORNER),
                egui::pos2(width, height),
            ),
            direction: ResizeDirection::SouthEast,
            cursor: CursorIcon::ResizeNwSe,
        },
        ResizeGrip {
            id: "south",
            rect: Rect::from_min_max(
                egui::pos2(RESIZE_CORNER, height - RESIZE_BORDER),
                egui::pos2(width - RESIZE_CORNER, height),
            ),
            direction: ResizeDirection::South,
            cursor: CursorIcon::ResizeVertical,
        },
        ResizeGrip {
            id: "south-west",
            rect: Rect::from_min_max(
                egui::pos2(0.0, height - RESIZE_CORNER),
                egui::pos2(RESIZE_CORNER, height),
            ),
            direction: ResizeDirection::SouthWest,
            cursor: CursorIcon::ResizeNeSw,
        },
        ResizeGrip {
            id: "west",
            rect: Rect::from_min_max(
                egui::pos2(0.0, RESIZE_CORNER),
                egui::pos2(RESIZE_BORDER, height - RESIZE_CORNER),
            ),
            direction: ResizeDirection::West,
            cursor: CursorIcon::ResizeHorizontal,
        },
    ]
}
