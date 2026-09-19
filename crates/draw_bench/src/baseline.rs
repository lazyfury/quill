//! Baseline files and regression verdicts.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::path::Path;

use crate::runner::BenchResult;

const HEADER: &str = "# draw_bench baseline v1";
const SEP: char = '\t';

/// How a current result compares to its baseline entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Slower than the baseline by more than the threshold.
    Regression,
    /// Faster than the baseline by more than the threshold.
    Improvement,
    /// Within the threshold either way.
    Stable,
    /// Not present in the baseline.
    New,
    /// Present in the baseline but not in the current run.
    Removed,
}

/// One benchmark's baseline-vs-current comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub name: String,
    pub baseline_ns: f64,
    pub current_ns: f64,
    /// `current / baseline` (or `0.0` for [`Verdict::Removed`]).
    pub ratio: f64,
    /// `ratio - 1.0` (or `-1.0` for [`Verdict::Removed`]).
    pub delta: f64,
    pub verdict: Verdict,
}

/// A named set of median per-iteration timings, persisted as plain text.
///
/// The format is intentionally trivial and diff-friendly:
///
/// ```text
/// # draw_bench baseline v1
/// scene/update_clean/1000	1203.4
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Baseline {
    entries: BTreeMap<String, f64>,
}

impl Baseline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a baseline from the medians of a set of results.
    pub fn from_results(results: &[BenchResult]) -> Self {
        let mut baseline = Self::new();
        for result in results {
            baseline.insert(result.name.clone(), result.median_ns());
        }
        baseline
    }

    pub fn insert(&mut self, name: impl Into<String>, median_ns: f64) {
        self.entries.insert(name.into(), median_ns);
    }

    pub fn get(&self, name: &str) -> Option<f64> {
        self.entries.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Deterministic text encoding (sorted by name).
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push('\n');
        for (name, ns) in &self.entries {
            out.push_str(name);
            out.push(SEP);
            out.push_str(&format!("{ns:.3}"));
            out.push('\n');
        }
        out
    }

    /// Parses the text format. Lines that are blank or start with `#` are
    /// ignored; any other malformed line is an error.
    pub fn from_text(text: &str) -> Result<Self, BaselineError> {
        let mut baseline = Self::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((name, value)) = line.split_once(SEP) else {
                return Err(BaselineError::Parse {
                    line: index + 1,
                    message: "expected `<name>\\t<median_ns>`".to_string(),
                });
            };
            let ns = value
                .trim()
                .parse::<f64>()
                .map_err(|_| BaselineError::Parse {
                    line: index + 1,
                    message: format!("invalid timing `{value}`"),
                })?;
            if name.is_empty() {
                return Err(BaselineError::Parse {
                    line: index + 1,
                    message: "empty benchmark name".to_string(),
                });
            }
            baseline.insert(name.to_string(), ns);
        }
        Ok(baseline)
    }

    /// Writes the baseline to `path`.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), BaselineError> {
        let mut file = std::fs::File::create(path)?;
        file.write_all(self.to_text().as_bytes())?;
        Ok(())
    }

    /// Reads a baseline from `path`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, BaselineError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_text(&text)
    }

    /// Compares results against this baseline at the given ratio threshold
    /// (e.g. `0.05` = 5%).
    ///
    /// Result order is preserved; baseline-only entries are appended in name
    /// order as [`Verdict::Removed`].
    pub fn compare(&self, results: &[BenchResult], threshold: f64) -> Vec<Comparison> {
        let threshold = threshold.max(0.0);
        let mut seen = Vec::new();
        let mut comparisons = Vec::with_capacity(results.len());

        for result in results {
            seen.push(result.name.as_str());
            let current_ns = result.median_ns();
            match self.get(&result.name) {
                Some(baseline_ns) if baseline_ns > 0.0 => {
                    let ratio = current_ns / baseline_ns;
                    let delta = ratio - 1.0;
                    // A tiny epsilon keeps a result that lands exactly on the
                    // threshold from flipping to a regression on rounding alone.
                    const EPS: f64 = 1e-9;
                    let verdict = if delta > threshold + EPS {
                        Verdict::Regression
                    } else if delta < -threshold - EPS {
                        Verdict::Improvement
                    } else {
                        Verdict::Stable
                    };
                    comparisons.push(Comparison {
                        name: result.name.clone(),
                        baseline_ns,
                        current_ns,
                        ratio,
                        delta,
                        verdict,
                    });
                }
                _ => comparisons.push(Comparison {
                    name: result.name.clone(),
                    baseline_ns: 0.0,
                    current_ns,
                    ratio: 0.0,
                    delta: 0.0,
                    verdict: Verdict::New,
                }),
            }
        }

        for (name, baseline_ns) in &self.entries {
            if seen.contains(&name.as_str()) {
                continue;
            }
            comparisons.push(Comparison {
                name: name.clone(),
                baseline_ns: *baseline_ns,
                current_ns: 0.0,
                ratio: 0.0,
                delta: -1.0,
                verdict: Verdict::Removed,
            });
        }

        comparisons
    }
}

/// Error produced while saving, loading or parsing a baseline.
#[derive(Debug)]
pub enum BaselineError {
    /// Underlying filesystem error.
    Io(std::io::Error),
    /// A malformed baseline line.
    Parse { line: usize, message: String },
}

impl fmt::Display for BaselineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BaselineError::Io(error) => write!(f, "{error}"),
            BaselineError::Parse { line, message } => {
                write!(f, "baseline line {line}: {message}")
            }
        }
    }
}

impl std::error::Error for BaselineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BaselineError::Io(error) => Some(error),
            BaselineError::Parse { .. } => None,
        }
    }
}

impl From<std::io::Error> for BaselineError {
    fn from(error: std::io::Error) -> Self {
        BaselineError::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Stats;

    fn results(pairs: &[(&str, f64)]) -> Vec<BenchResult> {
        pairs
            .iter()
            .map(|(name, ns)| BenchResult {
                name: (*name).to_string(),
                stats: Stats::from_samples(&[*ns], 1),
            })
            .collect()
    }

    #[test]
    fn text_round_trips() {
        let mut baseline = Baseline::new();
        baseline.insert("scene/paint/100", 1234.5);
        baseline.insert("ui/layout/10", 1.0);
        let text = baseline.to_text();
        let parsed = Baseline::from_text(&text).unwrap();
        assert_eq!(baseline, parsed);
        assert_eq!(parsed.get("scene/paint/100"), Some(1234.5));
    }

    #[test]
    fn rejects_malformed_lines() {
        let error = Baseline::from_text("no-tab-here\n").unwrap_err();
        assert!(matches!(error, BaselineError::Parse { line: 1, .. }));
        let error = Baseline::from_text("name\tnot-a-number\n").unwrap_err();
        assert!(matches!(error, BaselineError::Parse { line: 1, .. }));
    }

    #[test]
    fn compare_classifies_verdicts() {
        let baseline =
            Baseline::from_results(&results(&[("reg", 100.0), ("imp", 100.0), ("same", 100.0)]));
        let current = results(&[
            ("reg", 110.0),
            ("imp", 90.0),
            ("same", 102.0),
            ("brand", 50.0),
        ]);
        let comparisons = baseline.compare(&current, 0.05);
        let verdict = |name: &str| comparisons.iter().find(|c| c.name == name).unwrap().verdict;
        assert_eq!(verdict("reg"), Verdict::Regression);
        assert_eq!(verdict("imp"), Verdict::Improvement);
        assert_eq!(verdict("same"), Verdict::Stable);
        assert_eq!(verdict("brand"), Verdict::New);
    }

    #[test]
    fn compare_marks_removed_entries() {
        let baseline = Baseline::from_results(&results(&[("gone", 100.0)]));
        let comparisons = baseline.compare(&[], 0.05);
        assert_eq!(comparisons.len(), 1);
        assert_eq!(comparisons[0].verdict, Verdict::Removed);
        assert_eq!(comparisons[0].baseline_ns, 100.0);
    }

    #[test]
    fn threshold_boundaries_are_inclusive() {
        let baseline = Baseline::from_results(&results(&[("x", 100.0)]));
        // exactly +5% is stable (must exceed the threshold)
        let comparisons = baseline.compare(&results(&[("x", 105.0)]), 0.05);
        assert_eq!(comparisons[0].verdict, Verdict::Stable);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("draw_bench_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("baseline.txt");
        let mut baseline = Baseline::new();
        baseline.insert("a/b", 42.0);
        baseline.save(&path).unwrap();
        let loaded = Baseline::load(&path).unwrap();
        assert_eq!(loaded, baseline);
        std::fs::remove_file(&path).ok();
    }
}
