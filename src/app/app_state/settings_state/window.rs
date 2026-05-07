use super::{ScratchpadApp, WindowState};
use crate::app::services::settings_store::MIN_WINDOW_INNER_SIZE;
use eframe::egui;

impl ScratchpadApp {
    pub(crate) fn record_window_state(&mut self, ctx: &egui::Context) {
        let previous = self.app_settings.window_state.clone();
        let next = ctx.input(|input| window_state_from_viewport(input.viewport(), previous));
        self.app_settings.window_state = next;
    }
}

fn window_state_from_viewport(viewport: &egui::ViewportInfo, previous: WindowState) -> WindowState {
    let maximized = viewport.maximized.unwrap_or(previous.maximized);
    let minimized = viewport.minimized.unwrap_or(false);
    let fullscreen = viewport.fullscreen.unwrap_or(false);
    let mut next = WindowState {
        maximized,
        ..previous
    };

    if maximized || minimized || fullscreen {
        return next;
    }

    if let Some(inner_rect) = viewport.inner_rect {
        let size = inner_rect.size();
        if valid_window_size(size) {
            next.inner_size = Some([size.x, size.y]);
        }
    }

    if let Some(outer_rect) = viewport.outer_rect {
        let pos = outer_rect.min;
        if pos.x.is_finite() && pos.y.is_finite() {
            next.position = Some([pos.x, pos.y]);
        }
    }

    next
}

fn valid_window_size(size: egui::Vec2) -> bool {
    size.x.is_finite()
        && size.y.is_finite()
        && size.x >= MIN_WINDOW_INNER_SIZE[0]
        && size.y >= MIN_WINDOW_INNER_SIZE[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_state_capture_keeps_normal_geometry_while_maximized() {
        let previous = WindowState {
            position: Some([32.0, 48.0]),
            inner_size: Some([900.0, 700.0]),
            maximized: false,
        };
        let viewport = egui::ViewportInfo {
            inner_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1920.0, 1080.0),
            )),
            outer_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1920.0, 1080.0),
            )),
            maximized: Some(true),
            ..Default::default()
        };

        let next = window_state_from_viewport(&viewport, previous);

        assert!(next.maximized);
        assert_eq!(next.position, Some([32.0, 48.0]));
        assert_eq!(next.inner_size, Some([900.0, 700.0]));
    }

    #[test]
    fn window_state_capture_records_normal_geometry() {
        let viewport = egui::ViewportInfo {
            inner_rect: Some(egui::Rect::from_min_size(
                egui::pos2(108.0, 124.0),
                egui::vec2(980.0, 720.0),
            )),
            outer_rect: Some(egui::Rect::from_min_size(
                egui::pos2(100.0, 100.0),
                egui::vec2(996.0, 760.0),
            )),
            maximized: Some(false),
            ..Default::default()
        };

        let next = window_state_from_viewport(&viewport, WindowState::default());

        assert!(!next.maximized);
        assert_eq!(next.position, Some([100.0, 100.0]));
        assert_eq!(next.inner_size, Some([980.0, 720.0]));
    }
}
