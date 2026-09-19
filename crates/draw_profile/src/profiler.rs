use std::collections::VecDeque;

use crate::phase::Phase;
use crate::stats::{fps_from_ms, FrameStats, StageTimes};

/// Default history length: roughly two seconds at 60 FPS.
pub const DEFAULT_CAPACITY: usize = 120;

/// Aggregate view over the frames currently held by a [`Profiler`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameSummary {
    /// Number of frames averaged (≤ the profiler's capacity).
    pub frames: usize,
    pub avg_frame_ms: f32,
    pub min_frame_ms: f32,
    pub max_frame_ms: f32,
    /// Mean per-stage duration across the held frames.
    pub avg_stages: StageTimes,
    /// Largest `DrawCommand` count seen in the held frames.
    pub max_draw_commands: usize,
}

impl FrameSummary {
    /// FPS implied by the average frame time.
    pub fn fps(&self) -> f32 {
        fps_from_ms(self.avg_frame_ms)
    }

    /// Longest frame in the window, in milliseconds.
    pub fn worst_frame_ms(&self) -> f32 {
        self.max_frame_ms
    }

    /// Mean untimed remainder across the window.
    pub fn avg_untimed_ms(&self) -> f32 {
        (self.avg_frame_ms - self.avg_stages.sum()).max(0.0)
    }
}

/// A bounded, backend-neutral collector of [`FrameStats`].
///
/// It keeps the most recent [`capacity`](Profiler::capacity) frames in a ring
/// buffer and derives a [`FrameSummary`] on demand. The host owns the frame loop
/// and calls [`Profiler::record`]; the profiler never measures time itself.
///
/// A profiler can be disabled at runtime (e.g. the debug overlay is closed):
/// while disabled, [`record`](Profiler::record) is a no-op, so instrumentation
/// costs nothing on the hot path.
#[derive(Debug, Clone)]
pub struct Profiler {
    enabled: bool,
    capacity: usize,
    frames: VecDeque<FrameStats>,
    total_recorded: u64,
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiler {
    /// Creates an enabled profiler holding [`DEFAULT_CAPACITY`] frames.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Creates an enabled profiler with a custom history length.
    ///
    /// `capacity` is clamped to at least `1` so the ring buffer is always valid.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            enabled: true,
            capacity: capacity.max(1),
            frames: VecDeque::new(),
            total_recorded: 0,
        }
    }

    // -- enable / capacity -------------------------------------------------

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables recording. Disabling does not clear history.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Flips the enabled flag and returns the new state.
    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.enabled
    }

    /// Maximum number of frames retained.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    // -- recording ---------------------------------------------------------

    /// The index the next [`record`](Profiler::record)ed frame should carry.
    pub fn next_index(&self) -> u64 {
        self.total_recorded
    }

    /// Stores one frame, dropping the oldest when the history is full.
    ///
    /// Returns `true` when the frame was stored, `false` when the profiler is
    /// disabled.
    pub fn record(&mut self, stats: FrameStats) -> bool {
        if !self.enabled {
            return false;
        }
        self.total_recorded = self.total_recorded.saturating_add(1);
        if self.frames.len() == self.capacity {
            self.frames.pop_front();
        }
        self.frames.push_back(stats);
        true
    }

    /// Builds a frame from its parts, assigns it [`next_index`](Profiler::next_index)
    /// and records it. Returns `true` when stored.
    pub fn record_frame(
        &mut self,
        frame_ms: f32,
        stages: StageTimes,
        counters: crate::stats::FrameCounters,
    ) -> bool {
        let stats = FrameStats {
            index: self.next_index(),
            frame_ms,
            stages,
            counters,
        };
        self.record(stats)
    }

    // -- queries -----------------------------------------------------------

    /// Number of frames currently held (≤ [`capacity`](Profiler::capacity)).
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Total frames ever recorded, including ones already evicted.
    pub fn total_recorded(&self) -> u64 {
        self.total_recorded
    }

    pub fn clear(&mut self) {
        self.frames.clear();
    }

    /// Most recently recorded frame.
    pub fn last(&self) -> Option<&FrameStats> {
        self.frames.back()
    }

    /// Frame at `index` counted from the oldest held frame.
    pub fn frame_at(&self, index: usize) -> Option<&FrameStats> {
        self.frames.get(index)
    }

    /// Held frames, oldest first.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &FrameStats> {
        self.frames.iter()
    }

    /// Aggregates the held frames, or `None` when nothing has been recorded.
    pub fn summary(&self) -> Option<FrameSummary> {
        let frames = self.frames.len();
        if frames == 0 {
            return None;
        }

        let mut sum_frame = 0.0f32;
        let mut min_frame = f32::INFINITY;
        let mut max_frame = f32::NEG_INFINITY;
        let mut stage_sums = [0.0f32; 4];
        let mut max_draw_commands = 0usize;

        for frame in &self.frames {
            sum_frame += frame.frame_ms;
            min_frame = min_frame.min(frame.frame_ms);
            max_frame = max_frame.max(frame.frame_ms);
            for phase in Phase::ALL {
                stage_sums[phase.index()] += frame.stages.get(phase);
            }
            max_draw_commands = max_draw_commands.max(frame.counters.draw_commands);
        }

        let inv = 1.0 / frames as f32;
        let mut avg_stages = StageTimes::ZERO;
        for phase in Phase::ALL {
            avg_stages.set(phase, stage_sums[phase.index()] * inv);
        }

        Some(FrameSummary {
            frames,
            avg_frame_ms: sum_frame * inv,
            min_frame_ms: min_frame,
            max_frame_ms: max_frame,
            avg_stages,
            max_draw_commands,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::FrameCounters;

    fn frame(index: u64, ms: f32, commands: usize) -> FrameStats {
        let mut stats = FrameStats::new(index);
        stats.frame_ms = ms;
        stats.stages = StageTimes::new(ms * 0.1, ms * 0.2, ms * 0.3, ms * 0.4);
        stats.counters = FrameCounters::new(10, 5, commands, 1);
        stats
    }

    #[test]
    fn empty_profiler_has_no_summary() {
        let profiler = Profiler::new();
        assert!(profiler.is_empty());
        assert_eq!(profiler.len(), 0);
        assert!(profiler.summary().is_none());
        assert!(profiler.last().is_none());
        assert_eq!(profiler.capacity(), DEFAULT_CAPACITY);
    }

    #[test]
    fn ring_buffer_evicts_oldest_but_keeps_total() {
        let mut profiler = Profiler::with_capacity(3);
        for i in 0..5 {
            assert!(profiler.record(frame(i, 10.0, 1)));
        }
        assert_eq!(profiler.len(), 3);
        assert_eq!(profiler.total_recorded(), 5);
        // oldest held frame is index 2
        assert_eq!(profiler.frame_at(0).unwrap().index, 2);
        assert_eq!(profiler.last().unwrap().index, 4);
        assert_eq!(profiler.next_index(), 5);
    }

    #[test]
    fn capacity_is_clamped_to_at_least_one() {
        let mut profiler = Profiler::with_capacity(0);
        assert_eq!(profiler.capacity(), 1);
        profiler.record(frame(0, 1.0, 0));
        profiler.record(frame(1, 1.0, 0));
        assert_eq!(profiler.len(), 1);
        assert_eq!(profiler.last().unwrap().index, 1);
    }

    #[test]
    fn disabled_profiler_ignores_records() {
        let mut profiler = Profiler::new();
        profiler.set_enabled(false);
        assert!(!profiler.enabled());
        assert!(!profiler.record(frame(0, 16.0, 3)));
        assert!(profiler.is_empty());
        assert_eq!(profiler.total_recorded(), 0);

        assert!(profiler.toggle());
        assert!(profiler.record(frame(0, 16.0, 3)));
        assert_eq!(profiler.len(), 1);
    }

    #[test]
    fn record_frame_assigns_sequential_indices() {
        let mut profiler = Profiler::new();
        assert!(profiler.record_frame(8.0, StageTimes::ZERO, FrameCounters::ZERO));
        assert!(profiler.record_frame(9.0, StageTimes::ZERO, FrameCounters::ZERO));
        assert_eq!(profiler.frame_at(0).unwrap().index, 0);
        assert_eq!(profiler.frame_at(1).unwrap().index, 1);
    }

    #[test]
    fn summary_averages_min_max_and_counts() {
        let mut profiler = Profiler::with_capacity(8);
        profiler.record(frame(0, 10.0, 4));
        profiler.record(frame(1, 20.0, 9));
        profiler.record(frame(2, 30.0, 2));

        let summary = profiler.summary().unwrap();
        assert_eq!(summary.frames, 3);
        assert_eq!(summary.avg_frame_ms, 20.0);
        assert_eq!(summary.min_frame_ms, 10.0);
        assert_eq!(summary.max_frame_ms, 30.0);
        assert_eq!(summary.worst_frame_ms(), 30.0);
        assert_eq!(summary.max_draw_commands, 9);
        assert!((summary.fps() - 50.0).abs() < 1e-4);

        // stages were defined as fractions of frame_ms
        assert!((summary.avg_stages.update_ms - 2.0).abs() < 1e-4);
        assert!((summary.avg_stages.render_ms - 8.0).abs() < 1e-4);
        assert!((summary.avg_untimed_ms()).abs() < 1e-4);
    }

    #[test]
    fn clear_drops_history_but_not_counter() {
        let mut profiler = Profiler::new();
        profiler.record(frame(0, 1.0, 1));
        profiler.clear();
        assert!(profiler.is_empty());
        assert_eq!(profiler.total_recorded(), 1);
        assert!(profiler.summary().is_none());
    }
}
