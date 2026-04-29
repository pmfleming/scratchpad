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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollbarPolicy {
    AlwaysVisible,
    #[default]
    VisibleWhenNeeded,
    Hidden,
}
