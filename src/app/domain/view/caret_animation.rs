use crate::app::ui::editor_content::native_editor::CharCursor;
use eframe::egui;

const DURATION_SECONDS: f64 = 0.075;
const GEOMETRY_EPSILON: f32 = 0.1;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CaretAnimationState {
    cursor: Option<CharCursor>,
    from: Option<egui::Rect>,
    target: Option<egui::Rect>,
    started_at: f64,
    active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CaretAnimationFrame {
    pub(crate) rect: egui::Rect,
    pub(crate) active: bool,
}

impl CaretAnimationState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn update(
        &mut self,
        cursor: CharCursor,
        target: egui::Rect,
        now: f64,
        animate_target_change: bool,
        force_snap: bool,
    ) -> CaretAnimationFrame {
        let previous_cursor = self.cursor;
        let previous_target = self.target;
        let previous_visual = self.current_rect(now).unwrap_or(target);

        if force_snap || previous_cursor.is_none() {
            self.snap(cursor, target);
        } else if previous_cursor == Some(cursor) {
            if previous_target.is_none_or(|previous| !rect_approximately_equal(previous, target)) {
                // The logical cursor did not move, so its geometry changed because of scrolling,
                // wrapping, font metrics, or another layout change. Following that movement would
                // make the caret lag behind the document; keep it attached to the real position.
                self.snap(cursor, target);
            }
        } else if animate_target_change {
            if let Some(from) = animation_start_rect(previous_visual, target) {
                self.cursor = Some(cursor);
                self.from = Some(from);
                self.target = Some(target);
                self.started_at = now;
                self.active = true;
            } else {
                self.snap(cursor, target);
            }
        } else {
            self.snap(cursor, target);
        }

        let rect = self.current_rect(now).unwrap_or(target);
        CaretAnimationFrame {
            rect,
            active: self.active,
        }
    }

    fn snap(&mut self, cursor: CharCursor, target: egui::Rect) {
        self.cursor = Some(cursor);
        self.from = Some(target);
        self.target = Some(target);
        self.active = false;
    }

    fn current_rect(&mut self, now: f64) -> Option<egui::Rect> {
        let target = self.target?;
        if !self.active {
            return Some(target);
        }
        let from = self.from.unwrap_or(target);
        let progress = ((now - self.started_at) / DURATION_SECONDS).clamp(0.0, 1.0);
        if progress >= 1.0 {
            self.from = Some(target);
            self.active = false;
            return Some(target);
        }

        // Cubic ease-out starts moving immediately while settling gently at the destination.
        let progress = progress as f32;
        let eased = 1.0 - (1.0 - progress).powi(3);
        Some(lerp_rect(from, target, eased))
    }
}

fn rect_approximately_equal(left: egui::Rect, right: egui::Rect) -> bool {
    (left.min.x - right.min.x).abs() <= GEOMETRY_EPSILON
        && (left.min.y - right.min.y).abs() <= GEOMETRY_EPSILON
        && (left.max.x - right.max.x).abs() <= GEOMETRY_EPSILON
        && (left.max.y - right.max.y).abs() <= GEOMETRY_EPSILON
}

fn animation_start_rect(previous: egui::Rect, target: egui::Rect) -> Option<egui::Rect> {
    let row_height = previous.height().max(target.height()).max(1.0);
    let vertical_distance = (previous.center().y - target.center().y).abs();
    if vertical_distance > row_height * 1.5 {
        // Large jumps (page navigation, search, or programmatic movement) should be immediate.
        return None;
    }

    if vertical_distance > GEOMETRY_EPSILON {
        // For adjacent display rows, move vertically at the destination column instead of drawing
        // a diagonal caret through the intervening text.
        Some(egui::Rect::from_min_size(
            egui::pos2(target.min.x, previous.min.y),
            target.size(),
        ))
    } else {
        Some(egui::Rect::from_min_size(previous.min, target.size()))
    }
}

fn lerp_rect(from: egui::Rect, target: egui::Rect, amount: f32) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(
            egui::lerp(from.min.x..=target.min.x, amount),
            egui::lerp(from.min.y..=target.min.y, amount),
        ),
        target.size(),
    )
}

#[cfg(test)]
mod tests {
    use super::CaretAnimationState;
    use crate::app::ui::editor_content::native_editor::CharCursor;
    use eframe::egui;

    #[test]
    fn eases_between_keyboard_targets_on_the_same_row() {
        let mut animation = CaretAnimationState::default();
        let first = caret_rect(10.0, 20.0);
        let target = caret_rect(30.0, 20.0);
        assert_eq!(
            animation
                .update(CharCursor::new(1), first, 1.0, false, false)
                .rect,
            first
        );

        let started = animation.update(CharCursor::new(2), target, 2.0, true, false);
        assert!(started.active);
        assert_eq!(started.rect, first);

        let midway = animation.update(CharCursor::new(2), target, 2.0375, false, false);
        assert!(midway.active);
        assert!(midway.rect.left() > first.left());
        assert!(midway.rect.left() < target.left());

        let finished = animation.update(CharCursor::new(2), target, 2.1, false, false);
        assert!(!finished.active);
        assert_eq!(finished.rect, target);
    }

    #[test]
    fn adjacent_row_animation_uses_the_destination_column() {
        let mut animation = CaretAnimationState::default();
        animation.update(
            CharCursor::new(1),
            caret_rect(50.0, 20.0),
            1.0,
            false,
            false,
        );

        let target = caret_rect(12.0, 36.0);
        let started = animation.update(CharCursor::new(2), target, 2.0, true, false);

        assert!(started.active);
        assert_eq!(started.rect.left(), target.left());
        assert_eq!(started.rect.top(), 20.0);
    }

    #[test]
    fn large_cursor_jumps_and_layout_motion_snap() {
        let mut animation = CaretAnimationState::default();
        animation.update(
            CharCursor::new(1),
            caret_rect(10.0, 20.0),
            1.0,
            false,
            false,
        );

        let far_target = caret_rect(30.0, 100.0);
        let far_jump = animation.update(CharCursor::new(2), far_target, 2.0, true, false);
        assert!(!far_jump.active);
        assert_eq!(far_jump.rect, far_target);

        let scrolled_target = caret_rect(30.0, 92.0);
        let scrolled = animation.update(CharCursor::new(2), scrolled_target, 2.01, false, false);
        assert!(!scrolled.active);
        assert_eq!(scrolled.rect, scrolled_target);
    }

    #[test]
    fn forced_snap_cancels_an_active_animation() {
        let mut animation = CaretAnimationState::default();
        animation.update(
            CharCursor::new(1),
            caret_rect(10.0, 20.0),
            1.0,
            false,
            false,
        );
        let target = caret_rect(30.0, 20.0);
        assert!(
            animation
                .update(CharCursor::new(2), target, 2.0, true, false)
                .active
        );

        let snapped = animation.update(CharCursor::new(2), target, 2.01, false, true);
        assert!(!snapped.active);
        assert_eq!(snapped.rect, target);
    }

    fn caret_rect(x: f32, y: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(1.0, 16.0))
    }
}
