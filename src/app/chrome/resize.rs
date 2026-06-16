use crate::app::ui::widget_ids;
use eframe::egui::{self, CursorIcon, Rect, Vec2, pos2, viewport::ResizeDirection};

const RESIZE_BORDER: f32 = 6.0;
const RESIZE_CORNER: f32 = 18.0;

#[must_use]
pub fn handle_window_resize(ctx: &egui::Context, app_resize_grips_enabled: bool) -> bool {
    if !window_resize_enabled(ctx, app_resize_grips_enabled) {
        return false;
    }

    let screen_rect = ctx.input(|input| input.content_rect());
    let content_rect_changed = request_repaint_after_content_rect_change(ctx, screen_rect);
    maybe_begin_resize(ctx, screen_rect);

    content_rect_changed
}

pub fn show_window_resize_cursor(ctx: &egui::Context, app_resize_grips_enabled: bool) {
    if !window_resize_enabled(ctx, app_resize_grips_enabled) {
        return;
    }

    let Some(pointer_pos) = ctx.input(|input| input.pointer.hover_pos()) else {
        return;
    };

    let screen_rect = ctx.input(|input| input.content_rect());
    if let Some(grip) = resize_grips(screen_rect)
        .into_iter()
        .find(|grip| grip.rect.contains(pointer_pos))
    {
        ctx.output_mut(|output| output.cursor_icon = grip.cursor);
    }
}

fn window_resize_enabled(ctx: &egui::Context, app_resize_grips_enabled: bool) -> bool {
    app_resize_grips_enabled && !ctx.input(|input| input.viewport().maximized.unwrap_or(false))
}

fn request_repaint_after_content_rect_change(ctx: &egui::Context, screen_rect: Rect) -> bool {
    let rect_id = widget_ids::ctx_key("window_content_rect");
    let changed = ctx.data_mut(|data| {
        let previous = data.get_persisted::<Rect>(rect_id);
        data.insert_persisted(rect_id, screen_rect);
        previous.is_some_and(|previous| previous != screen_rect)
    });

    if changed {
        ctx.request_repaint();
    }

    changed
}

fn maybe_begin_resize(ctx: &egui::Context, screen_rect: Rect) {
    let Some(pointer_pos) = ctx.input(|input| {
        input
            .pointer
            .primary_pressed()
            .then(|| input.pointer.press_origin())
            .flatten()
    }) else {
        return;
    };

    if let Some(grip) = resize_grips(screen_rect)
        .into_iter()
        .find(|grip| grip.rect.contains(pointer_pos))
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(grip.direction));
        ctx.request_repaint();
    }
}

#[derive(Clone)]
struct ResizeGrip {
    rect: Rect,
    direction: ResizeDirection,
    cursor: CursorIcon,
}

fn resize_grips(screen_rect: Rect) -> [ResizeGrip; 8] {
    let rect = Rect::from_min_size(
        screen_rect.min,
        Vec2::new(
            screen_rect.width().max(RESIZE_CORNER * 2.0),
            screen_rect.height().max(RESIZE_CORNER * 2.0),
        ),
    );

    let corners = resize_corners(rect);
    let edges = resize_edges(rect);

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

fn resize_corners(rect: Rect) -> [ResizeGrip; 4] {
    [
        ResizeGrip {
            rect: Rect::from_min_max(
                rect.min,
                pos2(rect.min.x + RESIZE_CORNER, rect.min.y + RESIZE_CORNER),
            ),
            direction: ResizeDirection::NorthWest,
            cursor: CursorIcon::ResizeNwSe,
        },
        ResizeGrip {
            rect: Rect::from_min_max(
                pos2(rect.max.x - RESIZE_CORNER, rect.min.y),
                pos2(rect.max.x, rect.min.y + RESIZE_CORNER),
            ),
            direction: ResizeDirection::NorthEast,
            cursor: CursorIcon::ResizeNeSw,
        },
        ResizeGrip {
            rect: Rect::from_min_max(
                pos2(rect.max.x - RESIZE_CORNER, rect.max.y - RESIZE_CORNER),
                rect.max,
            ),
            direction: ResizeDirection::SouthEast,
            cursor: CursorIcon::ResizeNwSe,
        },
        ResizeGrip {
            rect: Rect::from_min_max(
                pos2(rect.min.x, rect.max.y - RESIZE_CORNER),
                pos2(rect.min.x + RESIZE_CORNER, rect.max.y),
            ),
            direction: ResizeDirection::SouthWest,
            cursor: CursorIcon::ResizeNeSw,
        },
    ]
}

fn resize_edges(rect: Rect) -> [ResizeGrip; 4] {
    [
        ResizeGrip {
            rect: Rect::from_min_max(
                pos2(rect.min.x + RESIZE_CORNER, rect.min.y),
                pos2(rect.max.x - RESIZE_CORNER, rect.min.y + RESIZE_BORDER),
            ),
            direction: ResizeDirection::North,
            cursor: CursorIcon::ResizeVertical,
        },
        ResizeGrip {
            rect: Rect::from_min_max(
                pos2(rect.max.x - RESIZE_BORDER, rect.min.y + RESIZE_CORNER),
                pos2(rect.max.x, rect.max.y - RESIZE_CORNER),
            ),
            direction: ResizeDirection::East,
            cursor: CursorIcon::ResizeHorizontal,
        },
        ResizeGrip {
            rect: Rect::from_min_max(
                pos2(rect.min.x + RESIZE_CORNER, rect.max.y - RESIZE_BORDER),
                pos2(rect.max.x - RESIZE_CORNER, rect.max.y),
            ),
            direction: ResizeDirection::South,
            cursor: CursorIcon::ResizeVertical,
        },
        ResizeGrip {
            rect: Rect::from_min_max(
                pos2(rect.min.x, rect.min.y + RESIZE_CORNER),
                pos2(rect.min.x + RESIZE_BORDER, rect.max.y - RESIZE_CORNER),
            ),
            direction: ResizeDirection::West,
            cursor: CursorIcon::ResizeHorizontal,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{RESIZE_BORDER, RESIZE_CORNER, resize_grips};
    use eframe::egui;
    use eframe::egui::viewport::ResizeDirection;

    #[test]
    fn east_resize_grip_tracks_screen_rect_right_edge() {
        let screen_rect =
            egui::Rect::from_min_size(egui::pos2(12.0, 8.0), egui::vec2(960.0, 640.0));

        let east = resize_grips(screen_rect)
            .into_iter()
            .find(|grip| grip.direction == ResizeDirection::East)
            .unwrap();

        assert_eq!(east.rect.right(), screen_rect.right());
        assert_eq!(east.rect.left(), screen_rect.right() - RESIZE_BORDER);
        assert_eq!(east.rect.top(), screen_rect.top() + RESIZE_CORNER);
        assert_eq!(east.rect.bottom(), screen_rect.bottom() - RESIZE_CORNER);
    }

    #[test]
    fn vertical_resize_grips_track_screen_rect_top_and_bottom_edges() {
        let screen_rect =
            egui::Rect::from_min_size(egui::pos2(12.0, 8.0), egui::vec2(960.0, 640.0));

        let north = resize_grips(screen_rect)
            .into_iter()
            .find(|grip| grip.direction == ResizeDirection::North)
            .unwrap();
        let south = resize_grips(screen_rect)
            .into_iter()
            .find(|grip| grip.direction == ResizeDirection::South)
            .unwrap();

        assert_eq!(north.rect.top(), screen_rect.top());
        assert_eq!(north.rect.bottom(), screen_rect.top() + RESIZE_BORDER);
        assert_eq!(north.rect.left(), screen_rect.left() + RESIZE_CORNER);
        assert_eq!(north.rect.right(), screen_rect.right() - RESIZE_CORNER);

        assert_eq!(south.rect.top(), screen_rect.bottom() - RESIZE_BORDER);
        assert_eq!(south.rect.bottom(), screen_rect.bottom());
        assert_eq!(south.rect.left(), screen_rect.left() + RESIZE_CORNER);
        assert_eq!(south.rect.right(), screen_rect.right() - RESIZE_CORNER);
    }
}
