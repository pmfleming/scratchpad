use eframe::egui;

/// Rendering availability is not the same as focus: an unfocused window may
/// still be visible, and Wayland may not report occlusion at all.
#[derive(Default)]
pub(crate) struct WindowLifecycle {
    renderable: Option<bool>,
    focused: bool,
}

impl WindowLifecycle {
    pub(super) fn update(&mut self, ctx: &egui::Context) {
        let (visible, focused) = ctx.input(|input| {
            (
                input.viewport().visible(),
                input.viewport().focused.unwrap_or(input.raw.focused),
            )
        });
        if self.observe(visible, focused) {
            // A single resume repaint, not a permanent timer or hidden-frame
            // warm-up. Native focus/unocclusion events wake eframe first.
            log::trace!(target: "scratchpad::window", "resume repaint visible={visible:?} focused={focused}");
            ctx.request_repaint();
        }
    }

    fn observe(&mut self, visible: Option<bool>, focused: bool) -> bool {
        // Match eframe's policy for platforms without visibility information.
        let renderable = visible.unwrap_or(true);
        let resumed = renderable && (self.renderable == Some(false) || (focused && !self.focused));
        self.renderable = Some(renderable);
        self.focused = focused;
        resumed
    }
}

#[cfg(test)]
mod tests {
    use super::WindowLifecycle;

    #[test]
    fn visibility_return_requests_one_repaint_without_focus() {
        let mut lifecycle = WindowLifecycle::default();
        assert!(!lifecycle.observe(Some(false), false));
        assert!(lifecycle.observe(Some(true), false));
        assert!(!lifecycle.observe(Some(true), false));
        assert!(!lifecycle.observe(Some(false), false));
        assert!(lifecycle.observe(Some(true), false));
    }

    #[test]
    fn unknown_visibility_uses_focus_return_without_continuous_repaints() {
        let mut lifecycle = WindowLifecycle::default();
        assert!(!lifecycle.observe(None, false));
        assert!(lifecycle.observe(None, true));
        assert!(!lifecycle.observe(None, true));
        assert!(!lifecycle.observe(None, false));
        assert!(lifecycle.observe(None, true));
    }

    #[test]
    fn hidden_focus_does_not_request_paint_until_renderable() {
        let mut lifecycle = WindowLifecycle::default();
        assert!(!lifecycle.observe(Some(false), true));
        assert!(!lifecycle.observe(Some(false), true));
        assert!(lifecycle.observe(None, true));
        assert!(!lifecycle.observe(None, true));
    }
}
