//! Sample statistics.

/// Summary statistics of a set of per-iteration samples, in nanoseconds.
///
/// All values are nanoseconds per iteration. `samples` is the number of timed
/// samples collected (not the number of routine calls); `iterations` is how
/// many routine calls each sample ran.
#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    /// Number of timed samples that were collected.
    pub samples: usize,
    /// Routine calls per timed sample.
    pub iterations: u64,
    /// Fastest observed sample.
    pub min_ns: f64,
    /// Slowest observed sample.
    pub max_ns: f64,
    /// Arithmetic mean of the samples.
    pub mean_ns: f64,
    /// 50th percentile (linear interpolation).
    pub median_ns: f64,
    /// Sample standard deviation (`n - 1` denominator; `0.0` for one sample).
    pub stddev_ns: f64,
    /// 90th percentile.
    pub p90_ns: f64,
    /// 95th percentile.
    pub p95_ns: f64,
    /// 99th percentile.
    pub p99_ns: f64,
}

impl Stats {
    /// Builds statistics from per-iteration samples (nanoseconds each).
    ///
    /// `iterations` is recorded as metadata only. An empty slice yields all
    /// zeros rather than panicking, so a mistuned bench cannot abort a suite.
    pub fn from_samples(samples: &[f64], iterations: u64) -> Self {
        let n = samples.len();
        if n == 0 {
            return Self {
                samples: 0,
                iterations,
                min_ns: 0.0,
                max_ns: 0.0,
                mean_ns: 0.0,
                median_ns: 0.0,
                stddev_ns: 0.0,
                p90_ns: 0.0,
                p95_ns: 0.0,
                p99_ns: 0.0,
            };
        }

        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mean = samples.iter().sum::<f64>() / n as f64;
        let variance = if n > 1 {
            samples
                .iter()
                .map(|x| {
                    let d = x - mean;
                    d * d
                })
                .sum::<f64>()
                / (n - 1) as f64
        } else {
            0.0
        };

        Self {
            samples: n,
            iterations,
            min_ns: sorted[0],
            max_ns: sorted[n - 1],
            mean_ns: mean,
            median_ns: percentile(&sorted, 50.0),
            stddev_ns: variance.sqrt(),
            p90_ns: percentile(&sorted, 90.0),
            p95_ns: percentile(&sorted, 95.0),
            p99_ns: percentile(&sorted, 99.0),
        }
    }

    /// Median throughput in iterations per second.
    pub fn per_second(&self) -> f64 {
        if self.median_ns > 0.0 {
            1e9 / self.median_ns
        } else {
            0.0
        }
    }

    /// Relative spread (`stddev / mean`), useful for judging trustworthiness.
    pub fn relative_stddev(&self) -> f64 {
        if self.mean_ns > 0.0 {
            self.stddev_ns / self.mean_ns
        } else {
            0.0
        }
    }
}

/// Linear-interpolated percentile of a pre-sorted (ascending) slice.
///
/// `p` is in `0.0..=100.0`. An empty slice returns `0.0`.
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let p = p.clamp(0.0, 100.0);
    let rank = (p / 100.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = rank - lo as f64;
        sorted[lo] * (1.0 - frac) + sorted[hi] * frac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_empty_and_single() {
        let empty = Stats::from_samples(&[], 7);
        assert_eq!(empty.samples, 0);
        assert_eq!(empty.median_ns, 0.0);

        let one = Stats::from_samples(&[42.0], 1);
        assert_eq!(one.samples, 1);
        assert_eq!(one.min_ns, 42.0);
        assert_eq!(one.max_ns, 42.0);
        assert_eq!(one.mean_ns, 42.0);
        assert_eq!(one.median_ns, 42.0);
        assert_eq!(one.stddev_ns, 0.0);
    }

    #[test]
    fn median_is_interpolated_for_even_counts() {
        let stats = Stats::from_samples(&[10.0, 20.0, 30.0, 40.0], 1);
        assert_eq!(stats.median_ns, 25.0);
        assert_eq!(stats.min_ns, 10.0);
        assert_eq!(stats.max_ns, 40.0);
        assert_eq!(stats.mean_ns, 25.0);
    }

    #[test]
    fn percentiles_are_monotonic() {
        let samples: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let stats = Stats::from_samples(&samples, 1);
        assert!(stats.p90_ns <= stats.p95_ns);
        assert!(stats.p95_ns <= stats.p99_ns);
        assert!(stats.median_ns <= stats.p90_ns);
        // nearest values with linear interpolation over 1..=100
        assert!((stats.median_ns - 50.5).abs() < 1e-9);
        assert!((stats.p99_ns - 99.01).abs() < 1e-9);
    }

    #[test]
    fn stddev_is_sample_deviation() {
        // mean 2.0, squared deviations 1,0,1 -> /(3-1)=1 -> sqrt=1
        let stats = Stats::from_samples(&[1.0, 2.0, 3.0], 1);
        assert!((stats.stddev_ns - 1.0).abs() < 1e-9);
        assert!((stats.relative_stddev() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn throughput_uses_median() {
        let stats = Stats::from_samples(&[1_000_000.0], 1);
        assert!((stats.per_second() - 1000.0).abs() < 1e-9);
    }
}
