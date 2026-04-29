use eframe::egui::Rect;

use super::anchor::ScrollAnchor;
use super::target::ScrollAlign;

/// A single, named scroll request. All scroll intents — wheel, page nav, search
/// jump, cursor reveal, scrollbar drag — flow through one resolution path on
/// `ScrollManager`. There is no other way to mutate scroll state.
#[derive(Clone, Copy, Debug)]
pub enum ScrollIntent {
    /// Mouse wheel delta (pixels). Positive y scrolls down (content moves up).
    Wheel { delta_x: f32, delta_y: f32 },
    /// Vertical scrollbar drag, target offset in pixels.
    ScrollbarTo { axis: Axis, offset_pixels: f32 },
    /// Move by a number of display rows (signed). Used by line-up / line-down.
    Lines(i32),
    /// Move by a number of viewports (signed). Used by PageUp / PageDown.
    Pages(i32),
    /// Scroll to absolute top.
    Top,
    /// Scroll to absolute bottom.
    Bottom,
    /// Reveal a target rect with the given alignment. Used for cursor reveal,
    /// search jumps, go-to-line.
    Reveal {
        rect: Rect,
        align_y: Option<ScrollAlign>,
        align_x: Option<ScrollAlign>,
    },
    /// Restore a previously-saved anchor (e.g. on view re-mount).
    RestoreAnchor(ScrollAnchor),
    /// Edge autoscroll request from a selection drag near the viewport edge.
    /// `velocity` is in pixels/second along the relevant axis.
    EdgeAutoscroll { axis: Axis, velocity: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}
