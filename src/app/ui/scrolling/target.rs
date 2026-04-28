use eframe::egui::{Rangef, Rect};

/// How a target rect should be aligned within the viewport when scrolled into
/// view.
///
/// `0.0` = leading edge (top/left), `1.0` = trailing edge (bottom/right),
/// `0.5` = center. `Min`/`Center`/`Max` are convenience aliases.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollAlign {
    Min,
    Center,
    Max,
    /// Reveal with margin: scroll only the minimum amount needed to make the
    /// target visible, leaving the given pixel margin from the nearest edge.
    NearestWithMargin(f32),
    Fraction(f32),
}

impl ScrollAlign {
    /// Compute the offset adjustment along one axis needed to align `target`
    /// inside `viewport`. All values are in scroll-content coordinates (i.e.
    /// before any current scroll has been applied).
    ///
    /// Returns the new scroll offset for that axis. `current_offset` is the
    /// current scroll offset; the visible window is
    /// `[current_offset, current_offset + viewport_size)`.
    pub fn resolve(
        self,
        target: Rangef,
        viewport_size: f32,
        content_size: f32,
        current_offset: f32,
    ) -> f32 {
        let max_offset = (content_size - viewport_size).max(0.0);
        let view_min = current_offset;
        let view_max = current_offset + viewport_size;
        let new = match self {
            ScrollAlign::Min => target.min,
            ScrollAlign::Max => target.max - viewport_size,
            ScrollAlign::Center => {
                let mid = 0.5 * (target.min + target.max);
                mid - 0.5 * viewport_size
            }
            ScrollAlign::Fraction(f) => {
                let f = f.clamp(0.0, 1.0);
                target.min - f * (viewport_size - (target.max - target.min))
            }
            ScrollAlign::NearestWithMargin(margin) => {
                if target.min < view_min + margin {
                    target.min - margin
                } else if target.max > view_max - margin {
                    target.max - viewport_size + margin
                } else {
                    current_offset
                }
            }
        };
        new.clamp(0.0, max_offset)
    }
}

/// Programmatic scroll target. Both axes optional so callers can request
/// vertical-only or horizontal-only reveals.
#[derive(Clone, Copy, Debug)]
pub struct ScrollTarget {
    pub rect: Rect,
    pub align_x: Option<ScrollAlign>,
    pub align_y: Option<ScrollAlign>,
}

impl ScrollTarget {
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            align_x: None,
            align_y: Some(ScrollAlign::NearestWithMargin(0.0)),
        }
    }

    pub fn with_y(mut self, align: ScrollAlign) -> Self {
        self.align_y = Some(align);
        self
    }

    pub fn with_x(mut self, align: ScrollAlign) -> Self {
        self.align_x = Some(align);
        self
    }
}

/// When the scrollbar should be drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarPolicy {
    AlwaysVisible,
    #[default]
    VisibleWhenNeeded,
    Hidden,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(min: f32, max: f32) -> Rangef {
        Rangef::new(min, max)
    }

    #[test]
    fn min_align_brings_target_top_to_viewport_top() {
        let new = ScrollAlign::Min.resolve(rng(500.0, 520.0), 400.0, 2_000.0, 0.0);
        assert_eq!(new, 500.0);
    }

    #[test]
    fn max_align_brings_target_bottom_to_viewport_bottom() {
        let new = ScrollAlign::Max.resolve(rng(500.0, 520.0), 400.0, 2_000.0, 0.0);
        // 520 - 400 = 120
        assert_eq!(new, 120.0);
    }

    #[test]
    fn center_align_centers_target_in_viewport() {
        let new = ScrollAlign::Center.resolve(rng(500.0, 520.0), 400.0, 2_000.0, 0.0);
        // mid 510, viewport half 200 → offset 310
        assert_eq!(new, 310.0);
    }

    #[test]
    fn nearest_with_margin_does_not_move_when_target_already_inside() {
        let cur = 100.0;
        // viewport [100, 500), target [200, 220], margin 10 → fits inside.
        let new =
            ScrollAlign::NearestWithMargin(10.0).resolve(rng(200.0, 220.0), 400.0, 2_000.0, cur);
        assert_eq!(new, cur);
    }

    #[test]
    fn nearest_with_margin_pulls_target_below_viewport_into_view() {
        let cur = 0.0;
        // viewport [0, 400), target [600, 620], margin 20 → new = 620 - 400 + 20 = 240.
        let new =
            ScrollAlign::NearestWithMargin(20.0).resolve(rng(600.0, 620.0), 400.0, 2_000.0, cur);
        assert_eq!(new, 240.0);
    }

    #[test]
    fn nearest_with_margin_pulls_target_above_viewport_into_view() {
        let cur = 800.0;
        // viewport [800, 1200), target [700, 720], margin 30 → new = 700 - 30 = 670.
        let new =
            ScrollAlign::NearestWithMargin(30.0).resolve(rng(700.0, 720.0), 400.0, 2_000.0, cur);
        assert_eq!(new, 670.0);
    }

    #[test]
    fn align_clamps_to_zero_when_target_near_top() {
        let new = ScrollAlign::Center.resolve(rng(0.0, 20.0), 400.0, 2_000.0, 500.0);
        assert_eq!(new, 0.0);
    }

    #[test]
    fn align_clamps_to_max_offset_when_target_near_bottom() {
        // content 1_000, viewport 400 → max_offset 600.
        let new = ScrollAlign::Center.resolve(rng(990.0, 1_000.0), 400.0, 1_000.0, 0.0);
        assert_eq!(new, 600.0);
    }

    #[test]
    fn fraction_align_places_target_at_specified_viewport_fraction() {
        // Place target at 25% from the top: f = 0.25.
        // new = target.min - f * (viewport - (max - min))
        //     = 600 - 0.25 * (400 - 20) = 600 - 95 = 505.
        let new = ScrollAlign::Fraction(0.25).resolve(rng(600.0, 620.0), 400.0, 2_000.0, 0.0);
        assert_eq!(new, 505.0);
    }
}
