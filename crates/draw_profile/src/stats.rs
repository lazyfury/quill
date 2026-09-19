use crate::phase::Phase;

/// Per-stage timing breakdown of a single frame, in milliseconds.
///
/// The four fields are intentionally explicit (rather than an array) so hosts
/// can set them by name; [`StageTimes::get`] / [`StageTimes::set`] provide
/// phase-indexed access for loops and the debug overlay.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StageTimes {
    pub update_ms: f32,
    pub layout_ms: f32,
    pub paint_ms: f32,
    pub render_ms: f32,
}

impl StageTimes {
    pub const ZERO: Self = Self {
        update_ms: 0.0,
        layout_ms: 0.0,
        paint_ms: 0.0,
        render_ms: 0.0,
    };

    pub const fn new(update_ms: f32, layout_ms: f32, paint_ms: f32, render_ms: f32) -> Self {
        Self {
            update_ms,
            layout_ms,
            paint_ms,
            render_ms,
        }
    }

    /// Reads one phase.
    pub fn get(&self, phase: Phase) -> f32 {
        match phase {
            Phase::Update => self.update_ms,
            Phase::Layout => self.layout_ms,
            Phase::Paint => self.paint_ms,
            Phase::Render => self.render_ms,
        }
    }

    /// Writes one phase. Negative values are clamped to `0.0` so a bad host
    /// clock cannot poison the summary.
    pub fn set(&mut self, phase: Phase, ms: f32) {
        let ms = if ms.is_finite() { ms.max(0.0) } else { 0.0 };
        match phase {
            Phase::Update => self.update_ms = ms,
            Phase::Layout => self.layout_ms = ms,
            Phase::Paint => self.paint_ms = ms,
            Phase::Render => self.render_ms = ms,
        }
    }

    /// Sum of the four stages, in milliseconds.
    pub fn sum(&self) -> f32 {
        self.update_ms + self.layout_ms + self.paint_ms + self.render_ms
    }

    /// The slowest stage, or `None` when every stage is exactly zero.
    pub fn dominant(&self) -> Option<Phase> {
        Phase::ALL
            .into_iter()
            .filter(|phase| self.get(*phase) > 0.0)
            .max_by(|a, b| {
                self.get(*a)
                    .partial_cmp(&self.get(*b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}

/// Cheap structural counts for a frame.
///
/// These are things a host can read without measuring anything, and they are
/// the raw material the inspection layer audits (e.g. command budgets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameCounters {
    /// Nodes visited in the `SceneTree`.
    pub scene_nodes: usize,
    /// `Control`s visited in the `Ui`.
    pub controls: usize,
    /// `DrawCommand`s emitted into this frame's `DrawList`(s).
    pub draw_commands: usize,
    /// Number of separate `DrawList`s submitted this frame.
    pub draw_lists: usize,
}

impl FrameCounters {
    pub const ZERO: Self = Self {
        scene_nodes: 0,
        controls: 0,
        draw_commands: 0,
        draw_lists: 0,
    };

    pub const fn new(
        scene_nodes: usize,
        controls: usize,
        draw_commands: usize,
        draw_lists: usize,
    ) -> Self {
        Self {
            scene_nodes,
            controls,
            draw_commands,
            draw_lists,
        }
    }

    /// Total number of tree entities observed (scene nodes + controls).
    pub const fn entities(&self) -> usize {
        self.scene_nodes + self.controls
    }
}

/// Everything recorded about one rendered frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameStats {
    /// Monotonic frame number assigned by the host / [`Profiler`](crate::Profiler).
    pub index: u64,
    /// Total wall-clock duration of the frame, in milliseconds.
    pub frame_ms: f32,
    /// Per-stage breakdown; may not sum exactly to `frame_ms` (untimed work).
    pub stages: StageTimes,
    /// Structural counts for the frame.
    pub counters: FrameCounters,
}

impl FrameStats {
    /// A frame with the given index and everything else zeroed.
    pub const fn new(index: u64) -> Self {
        Self {
            index,
            frame_ms: 0.0,
            stages: StageTimes::ZERO,
            counters: FrameCounters::ZERO,
        }
    }

    /// Instantaneous frames-per-second implied by `frame_ms`.
    ///
    /// Returns `0.0` for a non-positive or non-finite duration.
    pub fn fps(&self) -> f32 {
        fps_from_ms(self.frame_ms)
    }

    /// Untimed remainder: total frame time minus the sum of the timed stages.
    pub fn untimed_ms(&self) -> f32 {
        (self.frame_ms - self.stages.sum()).max(0.0)
    }
}

pub(crate) fn fps_from_ms(ms: f32) -> f32 {
    if ms.is_finite() && ms > 0.0 {
        1000.0 / ms
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_set_round_trip_for_every_phase() {
        let mut times = StageTimes::ZERO;
        for (i, phase) in Phase::ALL.into_iter().enumerate() {
            times.set(phase, (i + 1) as f32);
        }
        for (i, phase) in Phase::ALL.into_iter().enumerate() {
            assert_eq!(times.get(phase), (i + 1) as f32);
        }
        assert_eq!(times.sum(), 10.0);
    }

    #[test]
    fn set_clamps_invalid_values() {
        let mut times = StageTimes::ZERO;
        times.set(Phase::Paint, -5.0);
        assert_eq!(times.paint_ms, 0.0);
        times.set(Phase::Paint, f32::NAN);
        assert_eq!(times.paint_ms, 0.0);
        times.set(Phase::Paint, f32::INFINITY);
        assert_eq!(times.paint_ms, 0.0);
        times.set(Phase::Paint, 3.5);
        assert_eq!(times.paint_ms, 3.5);
    }

    #[test]
    fn dominant_phase_is_the_slowest_nonzero_one() {
        let times = StageTimes::new(1.0, 4.0, 2.0, 0.0);
        assert_eq!(times.dominant(), Some(Phase::Layout));
        assert_eq!(StageTimes::ZERO.dominant(), None);
    }

    #[test]
    fn fps_and_untimed_are_computed() {
        let mut frame = FrameStats::new(7);
        frame.frame_ms = 16.0;
        frame.stages = StageTimes::new(2.0, 3.0, 4.0, 5.0);
        assert_eq!(frame.untimed_ms(), 2.0);
        assert!((frame.fps() - 62.5).abs() < 1e-4);

        let zero = FrameStats::new(0);
        assert_eq!(zero.fps(), 0.0);
    }
}
