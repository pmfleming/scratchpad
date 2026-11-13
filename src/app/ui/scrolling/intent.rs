use eframe::egui::Rect;

use super::target::ScrollAlign;

/// A single, named scroll request. Scrollbar drags, cursor reveal and search
/// jumps flow through one resolution path on `ScrollManager`. There is no other
/// way to mutate scroll state.
#[derive(Clone, Copy, Debug)]
pub enum ScrollIntent {
    /// Scrollbar drag or pixel restore, target offset in pixels.
    ScrollbarTo { axis: Axis, offset_pixels: f32 },
    /// Reveal a target rect with the given alignment. Used for cursor reveal,
    /// search jumps, go-to-line.
    Reveal {
        rect: Rect,
        align_y: Option<ScrollAlign>,
        align_x: Option<ScrollAlign>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}
