//! Structural inspection of a `DrawList` plus the frame's counters/timing.
//!
//! The host calls [`inspect`] (or [`inspect_with`] with a custom
//! [`InspectionConfig`]) after painting a frame. The result is an
//! [`InspectionReport`]: a compact, severity-ranked list of [`Finding`]s that a
//! debug overlay can display and tests can assert on.
//!
//! Findings are aggregated by [`FindingCode`], so a frame with 300 degenerate
//! rectangles yields one `DegenerateRect` finding with `count == 300` instead of
//! 300 entries.

use draw_core::{Rect, Transform2D, Vec2};
use draw_render::{DrawCommand, DrawList};

use crate::stats::FrameStats;

/// How serious a [`Finding`] is. Ordered `Info < Warning < Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    /// All severities, least to most serious.
    pub const ALL: [Severity; 3] = [Severity::Info, Severity::Warning, Severity::Error];

    /// Numeric level (`Info = 0`), handy for sorting/aggregation.
    pub const fn level(self) -> u8 {
        match self {
            Severity::Info => 0,
            Severity::Warning => 1,
            Severity::Error => 2,
        }
    }

    /// Short label for the debug overlay.
    pub const fn label(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// The kind of problem a [`Finding`] describes.
///
/// The enum (rather than a raw string) keeps findings testable and lets the
/// overlay group/count them without parsing messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingCode {
    /// A `DrawList` with no commands.
    EmptyDrawList,
    /// `Save` / `Restore` did not balance inside one list.
    UnbalancedSaveRestore,
    /// `Restore` with nothing on the stack.
    UnmatchedRestore,
    /// Command count above the configured budget.
    CommandBudgetExceeded,
    /// Frame duration above the configured budget.
    FrameTimeBudgetExceeded,
    /// Scene nodes + controls above the configured budget.
    EntityBudgetExceeded,
    /// A rect with non-positive width/height.
    DegenerateRect,
    /// A circle with non-positive radius.
    DegenerateCircle,
    /// A stroke with non-positive width.
    DegenerateStroke,
    /// A clip rect with non-positive area.
    DegenerateClip,
    /// Non-finite (`NaN`/`inf`) geometry, transform or text metric.
    NonFiniteGeometry,
    /// Opacity outside `0.0..=1.0` (or non-finite).
    OpacityOutOfRange,
    /// Text with no visible glyphs.
    EmptyText,
}

impl FindingCode {
    /// Default severity used by [`InspectionReport::record`].
    pub const fn default_severity(self) -> Severity {
        match self {
            FindingCode::EmptyDrawList | FindingCode::EmptyText => Severity::Info,
            FindingCode::UnbalancedSaveRestore
            | FindingCode::UnmatchedRestore
            | FindingCode::NonFiniteGeometry => Severity::Error,
            _ => Severity::Warning,
        }
    }

    /// Stable short label (used by tests and the overlay).
    pub const fn label(self) -> &'static str {
        match self {
            FindingCode::EmptyDrawList => "empty-draw-list",
            FindingCode::UnbalancedSaveRestore => "unbalanced-save-restore",
            FindingCode::UnmatchedRestore => "unmatched-restore",
            FindingCode::CommandBudgetExceeded => "command-budget",
            FindingCode::FrameTimeBudgetExceeded => "frame-time-budget",
            FindingCode::EntityBudgetExceeded => "entity-budget",
            FindingCode::DegenerateRect => "degenerate-rect",
            FindingCode::DegenerateCircle => "degenerate-circle",
            FindingCode::DegenerateStroke => "degenerate-stroke",
            FindingCode::DegenerateClip => "degenerate-clip",
            FindingCode::NonFiniteGeometry => "non-finite-geometry",
            FindingCode::OpacityOutOfRange => "opacity-out-of-range",
            FindingCode::EmptyText => "empty-text",
        }
    }

    /// Whether exceeding a budget escalates the severity past `Warning`.
    fn escalates(self) -> bool {
        matches!(
            self,
            FindingCode::CommandBudgetExceeded
                | FindingCode::FrameTimeBudgetExceeded
                | FindingCode::EntityBudgetExceeded
        )
    }
}

/// One aggregated issue: a [`FindingCode`], the most serious severity seen, a
/// count of occurrences, and a human-readable first-occurrence message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub code: FindingCode,
    pub count: usize,
    pub message: String,
}

impl Finding {
    /// A one-line description suitable for a debug overlay row.
    pub fn summary(&self) -> String {
        if self.count > 1 {
            format!("{} (x{})", self.message, self.count)
        } else {
            self.message.clone()
        }
    }
}

/// Thresholds used by [`inspect_with`].
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

/// Inspects `list` and `stats` with the default [`InspectionConfig`].
pub fn inspect(list: &DrawList, stats: &FrameStats) -> InspectionReport {
    inspect_with(list, stats, &InspectionConfig::default())
}

/// Inspects `list` and `stats` against explicit thresholds.
pub fn inspect_with(
    list: &DrawList,
    stats: &FrameStats,
    config: &InspectionConfig,
) -> InspectionReport {
    let mut report = InspectionReport::new();
    inspect_frame(stats, config, &mut report);
    inspect_draw_list(list, config, &mut report);
    report
}

/// Inspects only the `DrawList` (geometry, commands, save/restore balance).
pub fn inspect_draw_list(
    list: &DrawList,
    config: &InspectionConfig,
    report: &mut InspectionReport,
) {
    if list.is_empty() {
        report.report(FindingCode::EmptyDrawList, "draw list has no commands");
        return;
    }

    check_budget(
        list.len(),
        config.max_draw_commands,
        FindingCode::CommandBudgetExceeded,
        "commands",
        report,
    );

    let mut save_depth: usize = 0;
    for command in list.commands() {
        match command {
            DrawCommand::Save => save_depth += 1,
            DrawCommand::Restore => {
                if save_depth == 0 {
                    report.report(
                        FindingCode::UnmatchedRestore,
                        "Restore without matching Save",
                    );
                } else {
                    save_depth -= 1;
                }
            }
            DrawCommand::SetTransform(transform) => check_transform(*transform, report),
            DrawCommand::SetOpacity(opacity) => {
                if !opacity.is_finite() || !(0.0..=1.0).contains(opacity) {
                    report.report(
                        FindingCode::OpacityOutOfRange,
                        format!("opacity {opacity} is outside 0.0..=1.0"),
                    );
                }
            }
            DrawCommand::ClipRect(rect) => {
                if check_rect(*rect, "clip rect", report) && is_degenerate(*rect) {
                    report.report(
                        FindingCode::DegenerateClip,
                        "clip rect has zero/negative area",
                    );
                }
            }
            DrawCommand::FillRect { rect, .. } => {
                if check_rect(*rect, "fill rect", report) && is_degenerate(*rect) {
                    report.report(
                        FindingCode::DegenerateRect,
                        "fill rect has zero/negative area",
                    );
                }
            }
            DrawCommand::StrokeRect { rect, width, .. } => {
                if check_rect(*rect, "stroke rect", report) && is_degenerate(*rect) {
                    report.report(
                        FindingCode::DegenerateRect,
                        "stroke rect has zero/negative area",
                    );
                }
                check_stroke_width(*width, "stroke rect", report);
            }
            DrawCommand::FillCircle { center, radius, .. } => {
                check_vec(*center, "circle center", report);
                check_radius(*radius, "fill circle", report);
            }
            DrawCommand::StrokeCircle {
                center,
                radius,
                width,
                ..
            } => {
                check_vec(*center, "circle center", report);
                check_radius(*radius, "stroke circle", report);
                check_stroke_width(*width, "stroke circle", report);
            }
            DrawCommand::DrawImage {
                destination,
                source,
                ..
            } => {
                if check_rect(*destination, "image destination", report)
                    && is_degenerate(*destination)
                {
                    report.report(
                        FindingCode::DegenerateRect,
                        "image destination has zero/negative area",
                    );
                }
                if let Some(source) = source {
                    if check_rect(*source, "image source", report) && is_degenerate(*source) {
                        report.report(
                            FindingCode::DegenerateRect,
                            "image source has zero/negative area",
                        );
                    }
                }
            }
            DrawCommand::DrawText {
                text,
                position,
                font_size,
                ..
            } => {
                check_vec(*position, "text position", report);
                if !font_size.is_finite() || *font_size <= 0.0 {
                    report.report(
                        FindingCode::NonFiniteGeometry,
                        format!("text font size {font_size} is not a positive number"),
                    );
                }
                if text.trim().is_empty() {
                    report.report(FindingCode::EmptyText, "text command has no visible glyphs");
                }
            }
        }
    }

    if save_depth != 0 {
        report.report(
            FindingCode::UnbalancedSaveRestore,
            format!("{save_depth} Save(s) without matching Restore"),
        );
    }
}

/// Inspects the frame's timing and structural counters.
pub fn inspect_frame(stats: &FrameStats, config: &InspectionConfig, report: &mut InspectionReport) {
    if stats.frame_ms.is_finite() && stats.frame_ms > config.max_frame_ms {
        let severity = if stats.frame_ms > config.max_frame_ms * 2.0 {
            Severity::Error
        } else {
            Severity::Warning
        };
        report.record(
            FindingCode::FrameTimeBudgetExceeded,
            severity,
            format!(
                "frame {:.1}ms exceeds {:.1}ms budget",
                stats.frame_ms, config.max_frame_ms
            ),
        );
    }

    check_budget(
        stats.counters.entities(),
        config.max_entities,
        FindingCode::EntityBudgetExceeded,
        "scene+ui entities",
        report,
    );
}

fn check_budget(
    value: usize,
    budget: usize,
    code: FindingCode,
    what: &str,
    report: &mut InspectionReport,
) {
    if value <= budget {
        return;
    }
    let severity = if code.escalates() && value > budget * 2 {
        Severity::Error
    } else {
        Severity::Warning
    };
    report.record(
        code,
        severity,
        format!("{value} {what} exceeds budget of {budget}"),
    );
}

fn check_transform(transform: Transform2D, report: &mut InspectionReport) {
    if !vec_finite(transform.x_axis)
        || !vec_finite(transform.y_axis)
        || !vec_finite(transform.origin)
    {
        report.report(
            FindingCode::NonFiniteGeometry,
            "transform contains non-finite values",
        );
    }
}

fn check_rect(rect: Rect, what: &str, report: &mut InspectionReport) -> bool {
    if !vec_finite(rect.origin) || !rect.size.width.is_finite() || !rect.size.height.is_finite() {
        report.report(
            FindingCode::NonFiniteGeometry,
            format!("{what} contains non-finite values"),
        );
        return false;
    }
    true
}

fn check_vec(value: Vec2, what: &str, report: &mut InspectionReport) {
    if !vec_finite(value) {
        report.report(
            FindingCode::NonFiniteGeometry,
            format!("{what} contains non-finite values"),
        );
    }
}

fn check_radius(radius: f32, what: &str, report: &mut InspectionReport) {
    if !radius.is_finite() {
        report.report(
            FindingCode::NonFiniteGeometry,
            format!("{what} radius is not finite"),
        );
    } else if radius <= 0.0 {
        report.report(
            FindingCode::DegenerateCircle,
            format!("{what} radius {radius} is not positive"),
        );
    }
}

fn check_stroke_width(width: f32, what: &str, report: &mut InspectionReport) {
    if !width.is_finite() {
        report.report(
            FindingCode::NonFiniteGeometry,
            format!("{what} stroke width is not finite"),
        );
    } else if width <= 0.0 {
        report.report(
            FindingCode::DegenerateStroke,
            format!("{what} stroke width {width} is not positive"),
        );
    }
}

fn vec_finite(value: Vec2) -> bool {
    value.x.is_finite() && value.y.is_finite()
}

fn is_degenerate(rect: Rect) -> bool {
    rect.size.width <= 0.0 || rect.size.height <= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Color, Size, Transform2D};
    use draw_render::{Paint, TextAlign};

    fn rect(w: f32, h: f32) -> Rect {
        Rect::from_min_size(Vec2::ZERO, Size::new(w, h))
    }

    fn stats(frame_ms: f32, entities: usize) -> FrameStats {
        let mut stats = FrameStats::new(0);
        stats.frame_ms = frame_ms;
        stats.counters.scene_nodes = entities;
        stats
    }

    fn clean_stats() -> FrameStats {
        stats(8.0, 10)
    }

    #[test]
    fn clean_list_produces_no_findings() {
        let list = DrawList::from(vec![
            DrawCommand::FillRect {
                rect: rect(10.0, 10.0),
                paint: Paint::solid(Color::RED),
            },
            DrawCommand::Save,
            DrawCommand::SetOpacity(0.5),
            DrawCommand::FillCircle {
                center: Vec2::new(5.0, 5.0),
                radius: 3.0,
                paint: Paint::solid(Color::BLUE),
            },
            DrawCommand::Restore,
            DrawCommand::DrawText {
                text: "hi".into(),
                position: Vec2::new(1.0, 2.0),
                font_size: 12.0,
                align: TextAlign::Left,
                paint: Paint::solid(Color::WHITE),
            },
        ]);
        let report = inspect(&list, &clean_stats());
        assert!(report.is_clean(), "unexpected: {:?}", report.findings());
        assert_eq!(report.max_severity(), None);
        assert_eq!(report.total(), 0);
    }

    #[test]
    fn empty_list_is_info() {
        let report = inspect(&DrawList::new(), &clean_stats());
        assert!(report.has(FindingCode::EmptyDrawList));
        assert_eq!(
            report.find(FindingCode::EmptyDrawList).unwrap().severity,
            Severity::Info
        );
        assert_eq!(report.max_severity(), Some(Severity::Info));
    }

    #[test]
    fn unbalanced_save_restore_is_error() {
        let list = DrawList::from(vec![DrawCommand::Save, DrawCommand::Save]);
        let report = inspect(&list, &clean_stats());
        let finding = report.find(FindingCode::UnbalancedSaveRestore).unwrap();
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.count, 1);
        assert_eq!(report.max_severity(), Some(Severity::Error));
    }

    #[test]
    fn unmatched_restore_is_error() {
        let list = DrawList::from(vec![DrawCommand::Restore]);
        let report = inspect(&list, &clean_stats());
        assert!(report.has(FindingCode::UnmatchedRestore));
    }

    #[test]
    fn degenerate_geometry_is_aggregated() {
        let list = DrawList::from(vec![
            DrawCommand::FillRect {
                rect: rect(0.0, 10.0),
                paint: Paint::solid(Color::RED),
            },
            DrawCommand::StrokeRect {
                rect: rect(10.0, -1.0),
                paint: Paint::solid(Color::RED),
                width: 1.0,
            },
            DrawCommand::FillCircle {
                center: Vec2::ZERO,
                radius: -2.0,
                paint: Paint::solid(Color::RED),
            },
            DrawCommand::StrokeRect {
                rect: rect(5.0, 5.0),
                paint: Paint::solid(Color::RED),
                width: 0.0,
            },
        ]);
        let report = inspect(&list, &clean_stats());
        assert_eq!(report.find(FindingCode::DegenerateRect).unwrap().count, 2);
        assert_eq!(report.find(FindingCode::DegenerateCircle).unwrap().count, 1);
        assert_eq!(report.find(FindingCode::DegenerateStroke).unwrap().count, 1);
        assert_eq!(report.max_severity(), Some(Severity::Warning));
    }

    #[test]
    fn non_finite_geometry_is_error() {
        let list = DrawList::from(vec![DrawCommand::FillRect {
            rect: rect(f32::NAN, 10.0),
            paint: Paint::solid(Color::RED),
        }]);
        let report = inspect(&list, &clean_stats());
        assert_eq!(
            report
                .find(FindingCode::NonFiniteGeometry)
                .unwrap()
                .severity,
            Severity::Error
        );
        // a non-finite rect is not *also* reported as degenerate
        assert!(!report.has(FindingCode::DegenerateRect));
    }

    #[test]
    fn opacity_out_of_range_is_warned() {
        let list = DrawList::from(vec![
            DrawCommand::SetOpacity(1.5),
            DrawCommand::SetOpacity(-0.1),
            DrawCommand::SetOpacity(f32::NAN),
        ]);
        let report = inspect(&list, &clean_stats());
        assert_eq!(
            report.find(FindingCode::OpacityOutOfRange).unwrap().count,
            3
        );
    }

    #[test]
    fn empty_text_is_info() {
        let list = DrawList::from(vec![DrawCommand::DrawText {
            text: "   ".into(),
            position: Vec2::ZERO,
            font_size: 12.0,
            align: TextAlign::Left,
            paint: Paint::solid(Color::WHITE),
        }]);
        let report = inspect(&list, &clean_stats());
        assert!(report.has(FindingCode::EmptyText));
        assert_eq!(report.max_severity(), Some(Severity::Info));
    }

    #[test]
    fn command_budget_escalates_from_warning_to_error() {
        let config = InspectionConfig {
            max_draw_commands: 2,
            ..InspectionConfig::default()
        };
        let list = |n: usize| {
            DrawList::from(
                (0..n)
                    .map(|_| DrawCommand::FillRect {
                        rect: rect(1.0, 1.0),
                        paint: Paint::solid(Color::RED),
                    })
                    .collect::<Vec<_>>(),
            )
        };

        let warning = inspect_with(&list(3), &clean_stats(), &config);
        assert_eq!(
            warning
                .find(FindingCode::CommandBudgetExceeded)
                .unwrap()
                .severity,
            Severity::Warning
        );

        let error = inspect_with(&list(5), &clean_stats(), &config);
        assert_eq!(
            error
                .find(FindingCode::CommandBudgetExceeded)
                .unwrap()
                .severity,
            Severity::Error
        );
    }

    #[test]
    fn frame_time_and_entity_budgets() {
        let list = DrawList::from(vec![DrawCommand::FillRect {
            rect: rect(1.0, 1.0),
            paint: Paint::solid(Color::RED),
        }]);
        let config = InspectionConfig {
            max_frame_ms: 10.0,
            max_entities: 5,
            ..InspectionConfig::default()
        };

        let report = inspect_with(&list, &stats(25.0, 100), &config);
        assert_eq!(
            report
                .find(FindingCode::FrameTimeBudgetExceeded)
                .unwrap()
                .severity,
            Severity::Error
        );
        assert!(report.has(FindingCode::EntityBudgetExceeded));
    }

    #[test]
    fn finding_summary_includes_count() {
        let mut report = InspectionReport::new();
        report.report(FindingCode::DegenerateRect, "bad rect");
        report.report(FindingCode::DegenerateRect, "bad rect");
        let finding = report.find(FindingCode::DegenerateRect).unwrap();
        assert_eq!(finding.count, 2);
        assert_eq!(finding.summary(), "bad rect (x2)");
    }

    #[test]
    fn severity_ordering_and_counts() {
        let mut report = InspectionReport::new();
        report.report(FindingCode::EmptyText, "x");
        report.report(FindingCode::DegenerateRect, "y");
        report.report(FindingCode::UnmatchedRestore, "z");
        assert_eq!(report.count_of(Severity::Info), 1);
        assert_eq!(report.count_of(Severity::Warning), 1);
        assert_eq!(report.count_of(Severity::Error), 1);
        assert_eq!(report.total(), 3);
        assert_eq!(report.max_severity(), Some(Severity::Error));
    }

    #[test]
    fn transform_and_clip_are_checked() {
        let list = DrawList::from(vec![
            DrawCommand::SetTransform(Transform2D::from_translation(Vec2::new(f32::INFINITY, 0.0))),
            DrawCommand::ClipRect(rect(0.0, 0.0)),
        ]);
        let report = inspect(&list, &clean_stats());
        assert!(report.has(FindingCode::NonFiniteGeometry));
        assert!(report.has(FindingCode::DegenerateClip));
    }
}
