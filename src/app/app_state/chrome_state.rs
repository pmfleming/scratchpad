use super::{AppSurface, CHROME_TRANSITION_FRAMES};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct ChromeTransitionTracker {
    frames_remaining: u8,
}

impl ChromeTransitionTracker {
    pub(crate) fn begin(&mut self) {
        self.frames_remaining = CHROME_TRANSITION_FRAMES;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.frames_remaining > 0
    }

    pub(crate) fn finish_frame(&mut self) {
        if self.frames_remaining > 0 {
            self.frames_remaining -= 1;
        }
    }
}

#[derive(Default)]
pub(crate) struct VerticalTabListState {
    pub(crate) open: bool,
    pub(crate) hide_deadline: Option<Instant>,
}

impl VerticalTabListState {
    pub(crate) fn reset_visibility(&mut self, keep_open: bool) {
        self.open = keep_open;
        self.hide_deadline = None;
    }

    pub(crate) fn clear_hide_deadline(&mut self) {
        self.hide_deadline = None;
    }

    pub(crate) fn delay_hide(&mut self, now: Instant, delay: Duration) {
        self.open = true;
        self.hide_deadline = Some(now + delay);
    }
}

pub(crate) struct ChromeState {
    pub(crate) transition: ChromeTransitionTracker,
    pub(crate) vertical_tabs: VerticalTabListState,
    active_surface: AppSurface,
    pending_status_bar_visible: Option<bool>,
}

impl Default for ChromeState {
    fn default() -> Self {
        Self {
            transition: ChromeTransitionTracker::default(),
            vertical_tabs: VerticalTabListState::default(),
            active_surface: AppSurface::Workspace,
            pending_status_bar_visible: None,
        }
    }
}

impl ChromeState {
    pub(crate) fn active_surface(&self) -> AppSurface {
        self.active_surface
    }

    pub(crate) fn set_active_surface(&mut self, surface: AppSurface) -> bool {
        let changed = self.active_surface != surface;
        self.active_surface = surface;
        changed
    }

    pub(crate) fn showing_settings(&self) -> bool {
        self.active_surface == AppSurface::Settings
    }

    pub(crate) fn activate_workspace_surface(&mut self) -> bool {
        self.set_active_surface(AppSurface::Workspace)
    }

    pub(crate) fn clear_pending_status_bar_visible(&mut self) {
        self.pending_status_bar_visible = None;
    }

    pub(crate) fn defer_status_bar_visible(&mut self, current: bool, next: bool) -> bool {
        self.pending_status_bar_visible = (current != next).then_some(next);
        self.pending_status_bar_visible.is_some()
    }

    pub(crate) fn take_pending_status_bar_visible(&mut self) -> Option<bool> {
        self.pending_status_bar_visible.take()
    }

    pub(crate) fn vertical_tabs_open(&self) -> bool {
        self.vertical_tabs.open
    }

    pub(crate) fn vertical_tabs_hide_deadline(&self) -> Option<Instant> {
        self.vertical_tabs.hide_deadline
    }
}
