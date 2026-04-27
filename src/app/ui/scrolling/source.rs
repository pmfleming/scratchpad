/// Input sources permitted to drive a `ScrollArea`. Bitflags-style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollSource {
    pub scroll_bar: bool,
    pub mouse_wheel: bool,
    pub drag: bool,
    /// Programmatic targets requested via `ScrollState::request_target` or
    /// passed in to `ScrollArea::scroll_to`.
    pub programmatic: bool,
}

impl ScrollSource {
    pub const ALL: Self = Self {
        scroll_bar: true,
        mouse_wheel: true,
        drag: true,
        programmatic: true,
    };

    pub const NONE: Self = Self {
        scroll_bar: false,
        mouse_wheel: false,
        drag: false,
        programmatic: false,
    };

    /// Editor default: everything except built-in egui drag-to-scroll. The
    /// editor handles its own pointer drags (selection + edge autoscroll), so
    /// the container must not steal them.
    pub const EDITOR: Self = Self {
        scroll_bar: true,
        mouse_wheel: true,
        drag: false,
        programmatic: true,
    };
}

impl Default for ScrollSource {
    fn default() -> Self {
        Self::ALL
    }
}
