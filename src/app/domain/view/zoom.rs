use super::EditorViewState;

pub const MIN_ZOOM_FACTOR: f32 = 0.25;
pub const MAX_ZOOM_FACTOR: f32 = 4.0;

#[must_use]
pub const fn default_zoom_factor() -> f32 {
    1.0
}

impl EditorViewState {
    #[must_use]
    pub fn zoom_factor(&self) -> f32 {
        self.zoom_factor
    }

    #[must_use]
    pub fn zoom_label(&self) -> String {
        format!("{:.0}%", self.zoom_factor * 100.0)
    }

    #[must_use]
    pub fn effective_font_size(&self, base_size: f32) -> f32 {
        base_size * self.zoom_factor
    }

    /// Zoom belongs to a view, never to its buffer or the global font setting.
    pub fn set_zoom_factor(&mut self, factor: f32) -> bool {
        let factor = if factor.is_finite() {
            factor.clamp(MIN_ZOOM_FACTOR, MAX_ZOOM_FACTOR)
        } else {
            default_zoom_factor()
        };
        if self.zoom_factor == factor {
            return false;
        }
        self.zoom_factor = factor;
        self.layout_cache.clear();
        self.clear_cursor_horizontal_goal();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::EditorViewState;

    #[test]
    fn zoom_is_bounded_and_relative_to_the_configured_font_size() {
        let mut view = EditorViewState::new(1);
        assert_eq!(view.zoom_label(), "100%");
        assert!(!view.set_zoom_factor(1.0));
        view.set_zoom_factor(1.25);
        assert_eq!(view.zoom_label(), "125%");
        assert_eq!(view.effective_font_size(14.0), 17.5);
        assert_eq!(view.effective_font_size(20.0), 25.0);
        for (factor, expected) in [
            (0.0, 0.25),
            (100.0, 4.0),
            (f32::NAN, 1.0),
            (f32::INFINITY, 1.0),
        ] {
            view.set_zoom_factor(factor);
            assert_eq!(view.zoom_factor(), expected);
        }
    }
}
