//! CLI configuration shared by every bench target.

use std::path::PathBuf;
use std::time::Duration;

use crate::runner::BenchOptions;

/// Threshold (as a ratio) above which a slower result is a [`Regression`].
///
/// [`Regression`]: crate::Verdict::Regression
pub const DEFAULT_THRESHOLD: f64 = 0.05;

/// Parsed command-line options for a bench run.
#[derive(Debug, Clone, PartialEq)]
pub struct RunConfig {
    /// Substring filter on benchmark names (`None` runs everything).
    pub filter: Option<String>,
    /// Baseline file to compare against, if any.
    pub baseline: Option<PathBuf>,
    /// Where to write the current results as a new baseline.
    pub save_baseline: Option<PathBuf>,
    /// Regression threshold as a ratio (e.g. `0.05` = 5%).
    pub threshold: f64,
    /// Sampling options.
    pub options: BenchOptions,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            filter: None,
            baseline: None,
            save_baseline: None,
            threshold: DEFAULT_THRESHOLD,
            options: BenchOptions::default(),
        }
    }
}

impl RunConfig {
    /// Parses [`std::env::args`], handling `--help` by printing usage and
    /// exiting successfully.
    pub fn from_env() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print!("{}", Self::usage());
            std::process::exit(0);
        }
        Self::parse(args)
    }

    /// Parses an argument list (without the program name).
    ///
    /// Unknown arguments are ignored so a stray flag does not abort long runs.
    pub fn parse<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut config = Self::default();
        let mut args = args.into_iter().map(Into::into);
        while let Some(arg) = args.next() {
            let mut value = || args.next();
            match arg.as_str() {
                "--filter" => config.filter = value(),
                "--baseline" => config.baseline = value().map(PathBuf::from),
                "--save-baseline" => config.save_baseline = value().map(PathBuf::from),
                "--threshold" => {
                    if let Some(percent) = value().and_then(|v| v.parse::<f64>().ok()) {
                        config.threshold = (percent / 100.0).max(0.0);
                    }
                }
                "--warmup-ms" => {
                    if let Some(ms) = value().and_then(|v| v.parse::<u64>().ok()) {
                        config.options.warmup = Duration::from_millis(ms);
                    }
                }
                "--sample-ms" => {
                    if let Some(ms) = value().and_then(|v| v.parse::<u64>().ok()) {
                        config.options.sample_time = Duration::from_millis(ms);
                    }
                }
                "--samples" => {
                    if let Some(n) = value().and_then(|v| v.parse::<usize>().ok()) {
                        config.options.samples = n.max(1);
                    }
                }
                "--max-iters" => {
                    if let Some(n) = value().and_then(|v| v.parse::<u64>().ok()) {
                        config.options.max_iters = n.max(1);
                    }
                }
                _ => {}
            }
        }
        config
    }

    /// Whether `name` passes [`RunConfig::filter`].
    pub fn matches(&self, name: &str) -> bool {
        match &self.filter {
            Some(filter) => name.contains(filter.as_str()),
            None => true,
        }
    }

    /// Usage text for `--help`.
    pub fn usage() -> String {
        format!(
            "draw_bench bench runner\n\
             \n\
             Usage: <bench target> [OPTIONS]\n\
             \n\
             Options:\n\
             \x20 --filter <substr>        Only run benchmarks whose name contains <substr>\n\
             \x20 --baseline <path>        Compare against a saved baseline file\n\
             \x20 --save-baseline <path>   Write the current results as a baseline file\n\
             \x20 --threshold <percent>    Regression threshold in percent (default {:.1})\n\
             \x20 --warmup-ms <ms>         Warmup per benchmark (default {})\n\
             \x20 --sample-ms <ms>         Target duration of one sample (default {})\n\
             \x20 --samples <n>            Timed samples per benchmark (default {})\n\
             \x20 --max-iters <n>          Cap on iterations per sample\n\
             \x20 -h, --help               Print this help\n",
            DEFAULT_THRESHOLD * 100.0,
            BenchOptions::default().warmup.as_millis(),
            BenchOptions::default().sample_time.as_millis(),
            BenchOptions::default().samples,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let config = RunConfig::default();
        assert_eq!(config.threshold, DEFAULT_THRESHOLD);
        assert!(config.filter.is_none());
        assert!(config.matches("anything"));
    }

    #[test]
    fn parses_all_flags() {
        let config = RunConfig::parse([
            "--filter",
            "scene/",
            "--baseline",
            "base.txt",
            "--save-baseline",
            "new.txt",
            "--threshold",
            "10",
            "--warmup-ms",
            "5",
            "--sample-ms",
            "7",
            "--samples",
            "9",
            "--max-iters",
            "123",
        ]);
        assert_eq!(config.filter.as_deref(), Some("scene/"));
        assert_eq!(config.baseline, Some(PathBuf::from("base.txt")));
        assert_eq!(config.save_baseline, Some(PathBuf::from("new.txt")));
        assert!((config.threshold - 0.10).abs() < 1e-12);
        assert_eq!(config.options.warmup, Duration::from_millis(5));
        assert_eq!(config.options.sample_time, Duration::from_millis(7));
        assert_eq!(config.options.samples, 9);
        assert_eq!(config.options.max_iters, 123);
        assert!(config.matches("scene/update_clean/100"));
        assert!(!config.matches("ui/layout/100"));
    }

    #[test]
    fn ignores_unknown_and_missing_values() {
        let config = RunConfig::parse(["--nope", "--samples"]);
        assert_eq!(config, RunConfig::default());
    }

    #[test]
    fn usage_lists_flags() {
        let usage = RunConfig::usage();
        for flag in ["--filter", "--baseline", "--threshold", "--samples"] {
            assert!(usage.contains(flag), "usage missing {flag}");
        }
    }
}
