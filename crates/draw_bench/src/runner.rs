//! The measured routine runner: warmup, calibration, sampling.

use std::time::{Duration, Instant};

use crate::config::RunConfig;
use crate::stats::Stats;

/// How a single benchmark is sampled.
///
/// The runner first warms the routine up, then estimates one iteration and picks
/// an iteration count so each timed sample runs for roughly
/// [`BenchOptions::sample_time`]. Total run time is about
/// `warmup + samples * sample_time` per benchmark.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchOptions {
    /// How long to run the routine before measuring.
    pub warmup: Duration,
    /// Target duration of one timed sample.
    pub sample_time: Duration,
    /// Number of timed samples to collect.
    pub samples: usize,
    /// Hard cap on the estimated iteration count, so a sub-nanosecond routine
    /// cannot ask the runner to loop forever.
    pub max_iters: u64,
}

impl Default for BenchOptions {
    fn default() -> Self {
        Self {
            warmup: Duration::from_millis(200),
            sample_time: Duration::from_millis(30),
            samples: 50,
            max_iters: 100_000_000,
        }
    }
}

/// One benchmark's measured result.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchResult {
    /// Stable, filterable name (e.g. `scene/update_clean/1000`).
    pub name: String,
    /// Statistics over the per-iteration samples.
    pub stats: Stats,
}

impl BenchResult {
    /// Median nanoseconds per iteration.
    pub fn median_ns(&self) -> f64 {
        self.stats.median_ns
    }

    /// Median throughput in iterations per second.
    pub fn per_second(&self) -> f64 {
        self.stats.per_second()
    }
}

/// Runs benchmarks under a fixed [`BenchOptions`] and optional name filter.
#[derive(Debug, Clone)]
pub struct BenchRunner {
    options: BenchOptions,
    filter: Option<String>,
}

impl BenchRunner {
    /// Creates a runner from a parsed [`RunConfig`].
    pub fn new(config: &RunConfig) -> Self {
        Self {
            options: config.options.clone(),
            filter: config.filter.clone(),
        }
    }

    /// Overrides the sampling options.
    pub fn with_options(mut self, options: BenchOptions) -> Self {
        self.options = options;
        self
    }

    pub fn options(&self) -> &BenchOptions {
        &self.options
    }

    /// Whether `name` passes the active filter (substring match; empty filter
    /// matches everything).
    pub fn matches(&self, name: &str) -> bool {
        match &self.filter {
            Some(filter) => name.contains(filter.as_str()),
            None => true,
        }
    }

    /// Warms up, calibrates and samples the routine.
    ///
    /// `setup` runs exactly once and is **not** timed. `routine` is called with
    /// the resulting state; the returned time is per routine call. Returns
    /// `None` when the name does not match the filter, so bench mains can skip
    /// filtered work cheaply.
    pub fn run<S, Setup, Routine>(
        &self,
        name: impl Into<String>,
        setup: Setup,
        mut routine: Routine,
    ) -> Option<BenchResult>
    where
        Setup: FnOnce() -> S,
        Routine: FnMut(&mut S),
    {
        let name = name.into();
        if !self.matches(&name) {
            return None;
        }

        let mut state = setup();
        let opts = &self.options;

        // Warm up caches, allocators and branch predictors.
        let warm_start = Instant::now();
        let mut warm_iters: u64 = 0;
        while warm_start.elapsed() < opts.warmup && warm_iters < opts.max_iters {
            routine(&mut state);
            warm_iters += 1;
        }

        // Estimate one call from a small batch, then size a sample.
        const CALIBRATION_ITERS: u64 = 16;
        let cal_start = Instant::now();
        for _ in 0..CALIBRATION_ITERS {
            routine(&mut state);
        }
        let single_ns = (cal_start.elapsed().as_nanos() as f64 / CALIBRATION_ITERS as f64).max(1.0);
        let target_ns = opts.sample_time.as_nanos() as f64;
        let iters = ((target_ns / single_ns).ceil() as u64).clamp(1, opts.max_iters);

        let mut samples = Vec::with_capacity(opts.samples);
        for _ in 0..opts.samples {
            let start = Instant::now();
            for _ in 0..iters {
                routine(&mut state);
            }
            let elapsed_ns = start.elapsed().as_nanos() as f64;
            samples.push(elapsed_ns / iters as f64);
        }

        Some(BenchResult {
            name,
            stats: Stats::from_samples(&samples, iters),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RunConfig {
        RunConfig {
            options: BenchOptions {
                warmup: Duration::from_millis(1),
                sample_time: Duration::from_millis(1),
                samples: 5,
                max_iters: 1_000_000,
            },
            ..RunConfig::default()
        }
    }

    #[test]
    fn records_requested_samples() {
        let mut config = test_config();
        config.filter = None;
        let runner = BenchRunner::new(&config);
        let result = runner.run("noop", || (), |_| {}).expect("runs");
        assert_eq!(result.name, "noop");
        assert_eq!(result.stats.samples, 5);
        assert!(result.stats.iterations >= 1);
    }

    #[test]
    fn setup_runs_once_and_state_is_threaded_through() {
        let mut config = test_config();
        config.filter = None;
        let runner = BenchRunner::new(&config);
        let mut setup_calls = 0;
        let result = runner.run(
            "count",
            || {
                setup_calls += 1;
                0u64
            },
            |state| *state += 1,
        );
        assert!(result.is_some());
        assert_eq!(setup_calls, 1);
    }

    #[test]
    fn filter_skips_non_matching_names() {
        let mut config = test_config();
        config.filter = Some("scene/".to_string());
        let runner = BenchRunner::new(&config);
        assert!(runner.run("ui/layout", || (), |_| {}).is_none());
        assert!(runner.matches("scene/update_clean/100"));
    }
}
