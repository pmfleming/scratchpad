use crate::app::theme::*;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};

pub struct AppIcons {
    pub menu: TextureHandle,
    pub close: TextureHandle,
    pub minimize: TextureHandle,
    pub maximize: TextureHandle,
    pub new_tab: TextureHandle,
}

impl AppIcons {
    pub fn load(ctx: &egui::Context) -> Self {
        Self {
            menu: load_texture(
                ctx,
                "menu-icon",
                include_bytes!("../assets/menu_button.png"),
            ),
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
            new_tab: load_texture(
                ctx,
                "new-icon",
                include_bytes!("../assets/new_tab_button.png"),
            ),
        }
    }
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
