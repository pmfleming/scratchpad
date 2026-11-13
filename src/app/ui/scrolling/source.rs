/// Input sources permitted to drive a `ScrollArea`. Bitflags-style.
///
/// There is deliberately no drag-to-scroll source: the editor handles its own
/// pointer drags (selection + edge autoscroll), so the container must not
/// steal them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollSource {
    pub scroll_bar: bool,
    pub mouse_wheel: bool,
}

impl ScrollSource {
    pub const ALL: Self = Self {
        scroll_bar: true,
        mouse_wheel: true,
    };
}

impl Default for ScrollSource {
    fn default() -> Self {
        Self::ALL
    }
}
