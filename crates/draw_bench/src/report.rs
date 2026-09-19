//! Human-readable tables for results and baseline comparisons.

use std::fmt::Write as _;

use crate::baseline::{Comparison, Verdict};
use crate::runner::BenchResult;

/// Formats a nanosecond duration with a suitable unit.
pub fn format_time(ns: f64) -> String {
    let ns = ns.max(0.0);
    if ns >= 1e9 {
        format!("{:.2}s", ns / 1e9)
    } else if ns >= 1e6 {
        format!("{:.2}ms", ns / 1e6)
    } else if ns >= 1e3 {
        format!("{:.2}µs", ns / 1e3)
    } else {
        format!("{:.1}ns", ns)
    }
}

/// Formats a count with a compact SI-style suffix.
pub fn format_count(value: f64) -> String {
    let value = value.max(0.0);
    if value >= 1e9 {
        format!("{:.2}G", value / 1e9)
    } else if value >= 1e6 {
        format!("{:.2}M", value / 1e6)
    } else if value >= 1e3 {
        format!("{:.2}K", value / 1e3)
    } else {
        format!("{value:.1}")
    }
}

fn name_width(results: &[BenchResult]) -> usize {
    results
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(20)
        .max(28)
}

/// Renders the results table (no trailing header/footer beyond the column line).
pub fn report(results: &[BenchResult]) -> String {
    let width = name_width(results);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<width$}  {:>10}  {:>10}  {:>10}  {:>10}  {:>10}  {:>12}",
        "benchmark",
        "median",
        "mean",
        "stddev",
        "min",
        "p95",
        "it/s",
        width = width
    );
    for result in results {
        let s = &result.stats;
        let _ = writeln!(
            out,
            "{:<width$}  {:>10}  {:>10}  {:>10}  {:>10}  {:>10}  {:>12}",
            result.name,
            format_time(s.median_ns),
            format_time(s.mean_ns),
            format_time(s.stddev_ns),
            format_time(s.min_ns),
            format_time(s.p95_ns),
            format_count(result.per_second()),
            width = width
        );
    }
    out
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Verdict::Regression => "REGRESSION",
            Verdict::Improvement => "improvement",
            Verdict::Stable => "stable",
            Verdict::New => "new",
            Verdict::Removed => "removed",
        })
    }
}

/// Renders the comparison table against a baseline plus a verdict tally.
pub fn comparison_report(comparisons: &[Comparison], threshold: f64) -> String {
    let width = comparisons
        .iter()
        .map(|c| c.name.len())
        .max()
        .unwrap_or(20)
        .max(28);

    let mut out = String::new();
    let _ = writeln!(
        out,
        "\nbaseline comparison (threshold {:.1}%)",
        threshold * 100.0
    );
    let _ = writeln!(
        out,
        "{:<width$}  {:>10}  {:>10}  {:>8}  {}",
        "benchmark",
        "baseline",
        "current",
        "change",
        "verdict",
        width = width
    );
    for c in comparisons {
        let _ = writeln!(
            out,
            "{:<width$}  {:>10}  {:>10}  {:>+7.1}%  {}",
            c.name,
            format_time(c.baseline_ns),
            format_time(c.current_ns),
            c.delta * 100.0,
            c.verdict,
            width = width
        );
    }

    let count = |verdict: Verdict| comparisons.iter().filter(|c| c.verdict == verdict).count();
    let _ = writeln!(
        out,
        "\n{} regression(s), {} improvement(s), {} stable, {} new, {} baseline-only",
        count(Verdict::Regression),
        count(Verdict::Improvement),
        count(Verdict::Stable),
        count(Verdict::New),
        count(Verdict::Removed),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Stats;

    fn result(name: &str, median_ns: f64) -> BenchResult {
        BenchResult {
            name: name.to_string(),
            stats: Stats::from_samples(&[median_ns], 1),
        }
    }

    #[test]
    fn formats_units() {
        assert_eq!(format_time(150.0), "150.0ns");
        assert_eq!(format_time(1_500.0), "1.50µs");
        assert_eq!(format_time(2_500_000.0), "2.50ms");
        assert_eq!(format_time(1_500_000_000.0), "1.50s");
        assert_eq!(format_count(1_500.0), "1.50K");
        assert_eq!(format_count(2_000_000.0), "2.00M");
    }

    #[test]
    fn report_contains_every_name() {
        let results = vec![result("a/1", 1000.0), result("b/2", 2000.0)];
        let text = report(&results);
        assert!(text.contains("a/1"));
        assert!(text.contains("b/2"));
        assert!(text.contains("median"));
        assert!(text.contains("it/s"));
    }

    #[test]
    fn comparison_report_tallies_verdicts() {
        let comparisons = vec![
            Comparison {
                name: "a".into(),
                baseline_ns: 100.0,
                current_ns: 120.0,
                delta: 0.20,
                ratio: 1.20,
                verdict: Verdict::Regression,
            },
            Comparison {
                name: "b".into(),
                baseline_ns: 100.0,
                current_ns: 80.0,
                delta: -0.20,
                ratio: 0.80,
                verdict: Verdict::Improvement,
            },
        ];
        let text = comparison_report(&comparisons, 0.05);
        assert!(text.contains("REGRESSION"));
        assert!(text.contains("improvement"));
        assert!(text.contains("1 regression(s)"));
        assert!(text.contains("1 improvement(s)"));
    }
}
