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

struct ResizeGrip {
    rect: Rect,
    direction: ResizeDirection,
    cursor: CursorIcon,
}

/// Where a grip lies along one axis of the window: a band of the given
/// thickness at either end, or the span between the corner grips.
#[derive(Clone, Copy)]
enum Band {
    Start(f32),
    Between,
    End(f32),
}

impl Band {
    fn span(self, min: f32, max: f32) -> (f32, f32) {
        match self {
            Self::Start(thickness) => (min, min + thickness),
            Self::Between => (min + RESIZE_CORNER, max - RESIZE_CORNER),
            Self::End(thickness) => (max - thickness, max),
        }
    }
}

/// Clockwise from the top-left corner: (direction, cursor, x band, y band).
const GRIPS: [(ResizeDirection, CursorIcon, Band, Band); 8] = {
    use Band::{Between, End, Start};
    use CursorIcon::{ResizeHorizontal, ResizeNeSw, ResizeNwSe, ResizeVertical};
    use ResizeDirection::{East, North, NorthEast, NorthWest, South, SouthEast, SouthWest, West};
    const C: f32 = RESIZE_CORNER;
    const B: f32 = RESIZE_BORDER;
    [
        (NorthWest, ResizeNwSe, Start(C), Start(C)),
        (North, ResizeVertical, Between, Start(B)),
        (NorthEast, ResizeNeSw, End(C), Start(C)),
        (East, ResizeHorizontal, End(B), Between),
        (SouthEast, ResizeNwSe, End(C), End(C)),
        (South, ResizeVertical, Between, End(B)),
        (SouthWest, ResizeNeSw, Start(C), End(C)),
        (West, ResizeHorizontal, Start(B), Between),
    ]
};

fn resize_grips(screen_rect: Rect) -> [ResizeGrip; 8] {
    let rect = Rect::from_min_size(
        screen_rect.min,
        Vec2::new(
            screen_rect.width().max(RESIZE_CORNER * 2.0),
            screen_rect.height().max(RESIZE_CORNER * 2.0),
        ),
    );
    GRIPS.map(|(direction, cursor, x_band, y_band)| {
        let (left, right) = x_band.span(rect.min.x, rect.max.x);
        let (top, bottom) = y_band.span(rect.min.y, rect.max.y);
        ResizeGrip {
            rect: Rect::from_min_max(pos2(left, top), pos2(right, bottom)),
            direction,
            cursor,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{RESIZE_BORDER as B, RESIZE_CORNER as C, resize_grips};
    use eframe::egui::{self, CursorIcon, viewport::ResizeDirection};

    #[test]
    fn grips_hug_each_edge_and_corner_of_the_window() {
        let screen = egui::Rect::from_min_size(egui::pos2(12.0, 8.0), egui::vec2(960.0, 640.0));
        let (l, t, r, b) = (screen.left(), screen.top(), screen.right(), screen.bottom());
        let expected = [
            (ResizeDirection::NorthWest, [l, t, l + C, t + C]),
            (ResizeDirection::North, [l + C, t, r - C, t + B]),
            (ResizeDirection::NorthEast, [r - C, t, r, t + C]),
            (ResizeDirection::East, [r - B, t + C, r, b - C]),
            (ResizeDirection::SouthEast, [r - C, b - C, r, b]),
            (ResizeDirection::South, [l + C, b - B, r - C, b]),
            (ResizeDirection::SouthWest, [l, b - C, l + C, b]),
            (ResizeDirection::West, [l, t + C, l + B, b - C]),
        ];
        let grips = resize_grips(screen);
        for (grip, (direction, [left, top, right, bottom])) in grips.iter().zip(expected) {
            assert_eq!(grip.direction, direction);
            assert_eq!(
                grip.rect,
                egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom)),
                "{direction:?}"
            );
        }
        assert_eq!(grips[0].cursor, CursorIcon::ResizeNwSe);
        assert_eq!(grips[3].cursor, CursorIcon::ResizeHorizontal);
    }
}
