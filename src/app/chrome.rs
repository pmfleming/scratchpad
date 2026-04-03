use crate::app::theme::*;
use eframe::egui::{
    self, Color32, CursorIcon, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2,
    viewport::ResizeDirection,
};

const RESIZE_BORDER: f32 = 6.0;
const RESIZE_CORNER: f32 = 18.0;

pub struct AppIcons {
    pub close: TextureHandle,
    pub minimize: TextureHandle,
    pub maximize: TextureHandle,
    pub open_file: TextureHandle,
    pub save: TextureHandle,
    pub search: TextureHandle,
    pub new_tab: TextureHandle,
}

impl AppIcons {
    pub fn load(ctx: &egui::Context) -> Self {
        Self {
            close: load_texture(
                ctx,
                "close-icon",
                include_bytes!("../assets/close_window_button.png"),
            ),
            minimize: load_texture(
                ctx,
                "min-icon",
                include_bytes!("../assets/minimize_button.png"),
            ),
            maximize: load_texture(
                ctx,
                "max-icon",
                include_bytes!("../assets/maximize_button.png"),
            ),
            open_file: load_texture(
                ctx,
                "open-file-icon",
                include_bytes!("../assets/open_file_button.png"),
            ),
            save: load_texture(
                ctx,
                "save-icon",
                include_bytes!("../assets/save_button.png"),
            ),
            search: load_texture(
                ctx,
                "search-icon",
                include_bytes!("../assets/search_button.png"),
            ),
            new_tab: load_texture(
                ctx,
                "new-icon",
                include_bytes!("../assets/new_tab_button.png"),
            ),
        }
    }
}

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
        .show(ctx, |ui| {
            // We don't set ui.set_min_size(screen_rect.size()) here anymore,
            // because that would make the whole area interactable if it was set to true.
            // Instead, we just interact with the specific grip rects.

            for grip in resize_grips(screen_rect.size()) {
                // We use ui.interact which is clip-rect aware.
                // Since the Area is not interactable, we are just placing "floating" interaction zones.
                let response = ui
                    .interact(grip.rect, ui.id().with(grip.id), Sense::click_and_drag())
                    .on_hover_cursor(grip.cursor);

                if response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(grip.direction));
                }
            }
        });
}

pub fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let image = image::load_from_memory(bytes)
        .unwrap_or_else(|error| panic!("failed to decode {name}: {error}"))
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    ctx.load_texture(name.to_owned(), color_image, TextureOptions::LINEAR)
}

pub fn icon_button(
    ui: &mut egui::Ui,
    texture: &TextureHandle,
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
    paint_texture(ui, rect, texture);

    response.on_hover_text(tooltip)
}

pub fn tab_button(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    close_icon: &TextureHandle,
) -> (egui::Response, egui::Response) {
    let size = Vec2::new(140.0, TAB_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let mut fill = if active {
        TAB_ACTIVE_BG
    } else {
        TAB_INACTIVE_BG
    };
    if response.hovered() && !active {
        fill = TAB_HOVER_BG;
    }

    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter()
        .rect_stroke(rect, 4.0, Stroke::new(1.0, BORDER));

    // Label
    let text_rect = Rect::from_min_max(
        rect.min + Vec2::new(8.0, 0.0),
        rect.max - Vec2::new(28.0, 0.0),
    );
    ui.painter().text(
        text_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Button.resolve(ui.style()),
        if active { TEXT_PRIMARY } else { TEXT_MUTED },
    );

    // Close button area (inside the tab)
    let close_rect = Rect::from_center_size(
        rect.right_center() - Vec2::new(14.0, 0.0),
        Vec2::new(18.0, 18.0),
    );

    // We use a sub-interaction for the close button
    let close_response = ui.interact(
        close_rect,
        ui.id().with(label).with("close"),
        Sense::click(),
    );

    if close_response.hovered() {
        ui.painter().rect_filled(close_rect, 2.0, CLOSE_HOVER_BG);
    }

    // Paint the close icon (using the window close image as requested)
    let icon_size = Vec2::new(10.0, 10.0);
    let icon_rect = Rect::from_center_size(close_rect.center(), icon_size);
    ui.painter().image(
        close_icon.id(),
        icon_rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );

    (response, close_response)
}

pub fn restore_button(
    ui: &mut egui::Ui,
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

    let back = Rect::from_min_size(rect.center() - Vec2::new(5.5, 5.5), Vec2::new(8.0, 8.0));
    let front = back.translate(Vec2::new(-3.0, 3.0));
    ui.painter()
        .rect_stroke(back, 0.0, Stroke::new(1.3, TEXT_PRIMARY));
    ui.painter()
        .rect_stroke(front, 0.0, Stroke::new(1.3, TEXT_PRIMARY));

    response.on_hover_text(tooltip)
}

fn paint_texture(ui: &egui::Ui, rect: Rect, texture: &TextureHandle) {
    let icon_rect = Rect::from_center_size(rect.center(), ICON_SIZE);
    ui.painter().image(
        texture.id(),
        icon_rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
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
            rect: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(RESIZE_CORNER, RESIZE_CORNER)),
            direction: ResizeDirection::NorthWest,
            cursor: CursorIcon::ResizeNwSe,
        },
        ResizeGrip {
            id: "north",
            rect: Rect::from_min_max(
                Pos2::new(RESIZE_CORNER, 0.0),
                Pos2::new(width - RESIZE_CORNER, RESIZE_BORDER),
            ),
            direction: ResizeDirection::North,
            cursor: CursorIcon::ResizeVertical,
        },
        ResizeGrip {
            id: "north-east",
            rect: Rect::from_min_max(
                Pos2::new(width - RESIZE_CORNER, 0.0),
                Pos2::new(width, RESIZE_CORNER),
            ),
            direction: ResizeDirection::NorthEast,
            cursor: CursorIcon::ResizeNeSw,
        },
        ResizeGrip {
            id: "east",
            rect: Rect::from_min_max(
                Pos2::new(width - RESIZE_BORDER, RESIZE_CORNER),
                Pos2::new(width, height - RESIZE_CORNER),
            ),
            direction: ResizeDirection::East,
            cursor: CursorIcon::ResizeHorizontal,
        },
        ResizeGrip {
            id: "south-east",
            rect: Rect::from_min_max(
                Pos2::new(width - RESIZE_CORNER, height - RESIZE_CORNER),
                Pos2::new(width, height),
            ),
            direction: ResizeDirection::SouthEast,
            cursor: CursorIcon::ResizeNwSe,
        },
        ResizeGrip {
            id: "south",
            rect: Rect::from_min_max(
                Pos2::new(RESIZE_CORNER, height - RESIZE_BORDER),
                Pos2::new(width - RESIZE_CORNER, height),
            ),
            direction: ResizeDirection::South,
            cursor: CursorIcon::ResizeVertical,
        },
        ResizeGrip {
            id: "south-west",
            rect: Rect::from_min_max(
                Pos2::new(0.0, height - RESIZE_CORNER),
                Pos2::new(RESIZE_CORNER, height),
            ),
            direction: ResizeDirection::SouthWest,
            cursor: CursorIcon::ResizeNeSw,
        },
        ResizeGrip {
            id: "west",
            rect: Rect::from_min_max(
                Pos2::new(0.0, RESIZE_CORNER),
                Pos2::new(RESIZE_BORDER, height - RESIZE_CORNER),
            ),
            direction: ResizeDirection::West,
            cursor: CursorIcon::ResizeHorizontal,
        },
    ]
}
