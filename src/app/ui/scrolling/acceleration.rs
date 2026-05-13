#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScrollAccelerationState {
    level: f32,
    last_time: f64,
    direction: i8,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScrollAccelerationConfig {
    pub(super) reset_after_seconds: f64,
    pub(super) ramp_per_second: f32,
    pub(super) ramp_per_pixel: f32,
    pub(super) max_multiplier: f32,
}

pub(super) fn accelerated_scroll_delta(
    state: &mut ScrollAccelerationState,
    now: f64,
    delta: f32,
    config: ScrollAccelerationConfig,
) -> f32 {
    if delta.abs() <= f32::EPSILON {
        return 0.0;
    }

    let direction = delta.signum() as i8;
    let dt = (now - state.last_time).max(0.0);
    let continues = state.direction == direction && dt <= config.reset_after_seconds;
    if continues {
        state.level = (state.level
            + dt as f32 * config.ramp_per_second
            + delta.abs() * config.ramp_per_pixel)
            .clamp(0.0, 1.0);
    } else {
        state.level = 0.0;
    }

    state.last_time = now;
    state.direction = direction;
    delta * acceleration_multiplier(state.level, config.max_multiplier)
}

fn acceleration_multiplier(level: f32, max_multiplier: f32) -> f32 {
    1.0 + (max_multiplier - 1.0).max(0.0) * level.clamp(0.0, 1.0).powi(2)
}

#[cfg(test)]
mod tests {
    use super::{ScrollAccelerationConfig, ScrollAccelerationState, accelerated_scroll_delta};

    const CONFIG: ScrollAccelerationConfig = ScrollAccelerationConfig {
        reset_after_seconds: 0.20,
        ramp_per_second: 3.0,
        ramp_per_pixel: 0.01,
        max_multiplier: 3.0,
    };

    #[test]
    fn repeated_scrolls_ramp_up() {
        let mut state = ScrollAccelerationState::default();

        let first = accelerated_scroll_delta(&mut state, 1.0, 10.0, CONFIG);
        let second = accelerated_scroll_delta(&mut state, 1.05, 10.0, CONFIG);

        assert_eq!(first, 10.0);
        assert!(second > first);
    }

    #[test]
    fn pause_resets_ramp() {
        let mut state = ScrollAccelerationState::default();

        let _ = accelerated_scroll_delta(&mut state, 1.0, 10.0, CONFIG);
        let _ = accelerated_scroll_delta(&mut state, 1.05, 10.0, CONFIG);
        let after_pause = accelerated_scroll_delta(&mut state, 1.4, 10.0, CONFIG);

        assert_eq!(after_pause, 10.0);
    }

    #[test]
    fn direction_change_resets_ramp() {
        let mut state = ScrollAccelerationState::default();

        let _ = accelerated_scroll_delta(&mut state, 1.0, 10.0, CONFIG);
        let _ = accelerated_scroll_delta(&mut state, 1.05, 10.0, CONFIG);
        let opposite = accelerated_scroll_delta(&mut state, 1.08, -10.0, CONFIG);

        assert_eq!(opposite, -10.0);
    }
}
