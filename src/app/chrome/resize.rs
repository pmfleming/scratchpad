use eframe::egui::{
    self, CursorIcon, Rect, Sense, Vec2, pos2, viewport::ResizeDirection,
};

const RESIZE_BORDER: f32 = 6.0;
const RESIZE_CORNER: f32 = 18.0;

pub fn handle_window_resize(ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    if maximized {
        return;
    }

    let screen_rect = ctx.input(|input| input.content_rect());
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

#[derive(Clone)]
struct ResizeGrip {
    id: &'static str,
    rect: Rect,
    direction: ResizeDirection,
    cursor: CursorIcon,
}

fn resize_grips(size: Vec2) -> [ResizeGrip; 8] {
    let width = size.x.max(RESIZE_CORNER * 2.0);
    let height = size.y.max(RESIZE_CORNER * 2.0);

    let corners = resize_corners(width, height);
    let edges = resize_edges(width, height);

    [
        corners[0].clone(),
        edges[0].clone(),
        corners[1].clone(),
        edges[1].clone(),
        corners[2].clone(),
        edges[2].clone(),
        corners[3].clone(),
        edges[3].clone(),
    ]
}

fn resize_corners(width: f32, height: f32) -> [ResizeGrip; 4] {
    [
        ResizeGrip {
            id: "north-west",
            rect: Rect::from_min_max(pos2(0.0, 0.0), pos2(RESIZE_CORNER, RESIZE_CORNER)),
            direction: ResizeDirection::NorthWest,
            cursor: CursorIcon::ResizeNwSe,
        },
        ResizeGrip {
            id: "north-east",
            rect: Rect::from_min_max(
                pos2(width - RESIZE_CORNER, 0.0),
                pos2(width, RESIZE_CORNER),
            ),
            direction: ResizeDirection::NorthEast,
            cursor: CursorIcon::ResizeNeSw,
        },
        ResizeGrip {
            id: "south-east",
            rect: Rect::from_min_max(
                pos2(width - RESIZE_CORNER, height - RESIZE_CORNER),
                pos2(width, height),
            ),
            direction: ResizeDirection::SouthEast,
            cursor: CursorIcon::ResizeNwSe,
        },
        ResizeGrip {
            id: "south-west",
            rect: Rect::from_min_max(
                pos2(0.0, height - RESIZE_CORNER),
                pos2(RESIZE_CORNER, height),
            ),
            direction: ResizeDirection::SouthWest,
            cursor: CursorIcon::ResizeNeSw,
        },
    ]
}

fn resize_edges(width: f32, height: f32) -> [ResizeGrip; 4] {
    [
        ResizeGrip {
            id: "north",
            rect: Rect::from_min_max(
                pos2(RESIZE_CORNER, 0.0),
                pos2(width - RESIZE_CORNER, RESIZE_BORDER),
            ),
            direction: ResizeDirection::North,
            cursor: CursorIcon::ResizeVertical,
        },
        ResizeGrip {
            id: "east",
            rect: Rect::from_min_max(
                pos2(width - RESIZE_BORDER, RESIZE_CORNER),
                pos2(width, height - RESIZE_CORNER),
            ),
            direction: ResizeDirection::East,
            cursor: CursorIcon::ResizeHorizontal,
        },
        ResizeGrip {
            id: "south",
            rect: Rect::from_min_max(
                pos2(RESIZE_CORNER, height - RESIZE_BORDER),
                pos2(width - RESIZE_CORNER, height),
            ),
            direction: ResizeDirection::South,
            cursor: CursorIcon::ResizeVertical,
        },
        ResizeGrip {
            id: "west",
            rect: Rect::from_min_max(
                pos2(0.0, RESIZE_CORNER),
                pos2(RESIZE_BORDER, height - RESIZE_CORNER),
            ),
            direction: ResizeDirection::West,
            cursor: CursorIcon::ResizeHorizontal,
        },
    ]
}
