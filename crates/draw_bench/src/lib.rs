//! `draw_bench` — a tiny, dependency-free benchmarking harness for quill.
//!
//! It exists because a profiler and a benchmark answer different questions:
//! the profiler ([`draw_profile`]) tells you **where** frame time goes, while a
//! benchmark tells you **whether** a change made a path faster or slower against
//! a stable baseline. This crate is the "stable baseline" half.
//!
//! It has no dependencies outside `std`, calls no clock outside [`Instant`], and
//! introduces no randomness, so a run is deterministic given the same machine,
//! toolchain and inputs. That makes it safe to pin as a regression gate.
//!
//! # Pieces
//!
//! - [`Stats`] — sample count, min/max/mean/median/stddev and p90/p95/p99.
//! - [`BenchRunner`] — warms up, calibrates the iteration count, then collects
//!   [`BenchOptions::samples`] timed samples and returns a [`BenchResult`].
//! - [`report`] — human-readable tables for results and baseline comparisons.
//! - [`Baseline`] / [`Comparison`] / [`Verdict`] — a text baselines file plus
//!   regression verdicts at a configurable threshold.
//! - [`RunConfig`] — CLI parsing (`--filter`, `--baseline`, `--save-baseline`,
//!   `--threshold`, sample tuning) shared by every bench target.
//! - [`finish`] — the standard tail of a bench `main`: print, save/compare,
//!   return an exit code.
//!
//! # Example
//!
//! ```no_run
//! use draw_bench::{finish, BenchRunner, RunConfig};
//!
//! fn main() {
//!     let config = RunConfig::from_env();
//!     let runner = BenchRunner::new(&config);
//!     let mut results = Vec::new();
//!     if let Some(r) = runner.run("sum/1000", || (), |_| {
//!         let sum: u64 = (0..1000).sum();
//!         draw_bench::black_box(sum);
//!     }) {
//!         results.push(r);
//!     }
//!     std::process::exit(finish(&config, &results));
//! }
//! ```

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_bench";

mod baseline;
mod config;
mod report;
mod runner;
mod stats;

pub use baseline::{Baseline, Comparison, Verdict};
pub use config::RunConfig;
pub use report::{comparison_report, format_count, format_time, report};
pub use runner::{BenchOptions, BenchResult, BenchRunner};
pub use stats::Stats;

/// Opaque barrier that stops the optimizer from deleting the work under test.
///
/// Thin re-export of [`std::hint::black_box`], so callers have one import.
#[inline(always)]
pub fn black_box<T>(value: T) -> T {
    std::hint::black_box(value)
}

/// Prints the report, saves/loads a baseline, and returns a process exit code
/// (`1` when a [`Verdict::Regression`] is found against the loaded baseline).
///
/// This is the shared tail of a bench `main`; keeping it here means every bench
/// target reports and gates regressions identically.
pub fn finish(config: &RunConfig, results: &[BenchResult]) -> i32 {
    if results.is_empty() {
        println!("no benchmarks matched the filter");
        return 0;
    }

    print!("{}", report(results));

    if let Some(path) = &config.save_baseline {
        let baseline = Baseline::from_results(results);
        match baseline.save(path) {
            Ok(()) => println!(
                "\nsaved baseline: {} ({} entries)",
                path.display(),
                baseline.len()
            ),
            Err(error) => eprintln!(
                "\nwarning: could not save baseline {}: {error}",
                path.display()
            ),
        }
    }

    let Some(path) = &config.baseline else {
        return 0;
    };

    let baseline = match Baseline::load(path) {
        Ok(baseline) => baseline,
        Err(error) => {
            eprintln!(
                "warning: could not load baseline {}: {error}",
                path.display()
            );
            return 0;
        }
    };

    let comparisons = baseline.compare(results, config.threshold);
    print!("{}", comparison_report(&comparisons, config.threshold));
    i32::from(comparisons.iter().any(|c| c.verdict == Verdict::Regression))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_bench");
    }
}
