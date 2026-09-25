//! A fixed-timestep accumulator for host loops.
//!
//! A host measures the real frame time, feeds it to [`FixedTimestep::advance`],
//! runs the returned number of [`SceneTree::physics_process`](draw_scene::SceneTree::physics_process)
//! steps, and uses the returned `alpha` to interpolate rendering between the
//! previous and current physics states. The accumulator clamps a long stall so
//! it cannot spiral into an unbounded number of steps.

/// The result of advancing a [`FixedTimestep`] by one real frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// Fixed steps to run for this real frame.
    pub steps: u32,
    /// Fraction (`0.0..=1.0`) of the next step already elapsed — the render
    /// interpolation alpha.
    pub alpha: f32,
}

/// Fixed-step accumulator: converts a variable real `dt` into whole steps plus
/// an interpolation alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedTimestep {
    step: f32,
    accumulator: f32,
    max_steps: u32,
}

impl FixedTimestep {
    /// A clock at `hz` steps per second (clamped to a positive rate).
    pub fn from_hz(hz: f32) -> Self {
        Self::new(1.0 / hz.max(f32::EPSILON))
    }

    /// A clock with `step` seconds per step (clamped to a positive duration).
    pub fn new(step: f32) -> Self {
        Self {
            step: step.max(f32::EPSILON),
            accumulator: 0.0,
            max_steps: 8,
        }
    }

    /// Seconds per fixed step.
    pub fn step(&self) -> f32 {
        self.step
    }

    /// Sets the maximum steps one [`FixedTimestep::advance`] may return
    /// (clamped to at least 1).
    pub fn max_steps(mut self, max_steps: u32) -> Self {
        self.max_steps = max_steps.max(1);
        self
    }

    /// The current interpolation alpha (`accumulator / step`).
    pub fn alpha(&self) -> f32 {
        (self.accumulator / self.step).clamp(0.0, 1.0)
    }

    /// Drops the pending fraction.
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
    }

    /// Adds `real_dt` and returns how many fixed steps to run plus the alpha.
    ///
    /// A very large `real_dt` is clamped to `step * max_steps`; if the
    /// accumulator still overflows, the excess is dropped.
    pub fn advance(&mut self, real_dt: f32) -> Tick {
        let real_dt = real_dt.max(0.0);
        let max_time = self.step * self.max_steps as f32;
        self.accumulator += real_dt.min(max_time);

        let mut steps = (self.accumulator / self.step + 1e-6).floor() as u32;
        if steps > self.max_steps {
            steps = self.max_steps;
            self.accumulator = 0.0;
        } else {
            self.accumulator -= steps as f32 * self.step;
            if self.accumulator < 0.0 {
                self.accumulator = 0.0;
            }
        }
        Tick {
            steps,
            alpha: self.alpha(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_hz_sets_the_step_duration() {
        assert!((FixedTimestep::from_hz(50.0).step() - 0.02).abs() < 1e-6);
    }

    #[test]
    fn advance_returns_whole_steps_and_an_alpha() {
        let mut clock = FixedTimestep::new(0.02);
        let tick = clock.advance(0.05);
        assert_eq!(tick.steps, 2);
        assert!((tick.alpha - 0.5).abs() < 1e-4, "alpha {}", tick.alpha);

        // The leftover 10 ms carries into the next frame.
        let tick = clock.advance(0.01);
        assert_eq!(tick.steps, 1);
        assert!(tick.alpha.abs() < 1e-4, "alpha {}", tick.alpha);
    }

    #[test]
    fn a_long_stall_is_clamped_to_max_steps() {
        let mut clock = FixedTimestep::new(0.02).max_steps(4);
        let tick = clock.advance(10.0);
        assert_eq!(tick.steps, 4);
        assert_eq!(tick.alpha, 0.0);
    }

    #[test]
    fn reset_drops_the_pending_fraction() {
        let mut clock = FixedTimestep::new(0.02);
        clock.advance(0.03);
        assert!(clock.alpha() > 0.0);
        clock.reset();
        assert_eq!(clock.alpha(), 0.0);
    }
}
