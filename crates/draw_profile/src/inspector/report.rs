//! The inspection report and its thresholds.

use super::finding::{Finding, FindingCode, Severity};

/// Thresholds used by [`inspect_with`](crate::inspect_with).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InspectionConfig {
    /// Warn when a single frame's `DrawList` exceeds this many commands.
    pub max_draw_commands: usize,
    /// Warn when a frame takes longer than this (milliseconds).
    pub max_frame_ms: f32,
    /// Warn when scene nodes + controls exceed this many.
    pub max_entities: usize,
}

impl Default for InspectionConfig {
    fn default() -> Self {
        Self {
            max_draw_commands: 2048,
            max_frame_ms: 16.7,
            max_entities: 10_000,
        }
    }
}

/// The outcome of inspecting one frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InspectionReport {
    findings: Vec<Finding>,
}

impl InspectionReport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Aggregates by code: bumps `count` and keeps the most serious severity.
    pub fn record(&mut self, code: FindingCode, severity: Severity, message: impl Into<String>) {
        if let Some(existing) = self.findings.iter_mut().find(|f| f.code == code) {
            existing.count += 1;
            existing.severity = existing.severity.max(severity);
        } else {
            self.findings.push(Finding {
                severity,
                code,
                count: 1,
                message: message.into(),
            });
        }
    }

    /// Records using the code's [`default_severity`](FindingCode::default_severity).
    pub fn report(&mut self, code: FindingCode, message: impl Into<String>) {
        self.record(code, code.default_severity(), message);
    }

    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// An empty report means the frame is clean.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn len(&self) -> usize {
        self.findings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    /// Total occurrences across all findings (sum of `count`).
    pub fn total(&self) -> usize {
        self.findings.iter().map(|f| f.count).sum()
    }

    /// Number of findings at exactly `severity`.
    pub fn count_of(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }

    /// Most serious severity present, or `None` when clean.
    pub fn max_severity(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }

    /// The aggregated finding for `code`, if any.
    pub fn find(&self, code: FindingCode) -> Option<&Finding> {
        self.findings.iter().find(|f| f.code == code)
    }

    /// Whether `code` is present.
    pub fn has(&self, code: FindingCode) -> bool {
        self.find(code).is_some()
    }
}
