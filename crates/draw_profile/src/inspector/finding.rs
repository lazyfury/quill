//! Finding types: severity, code and an aggregated finding.

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
    /// Default severity used by [`InspectionReport::record`](crate::InspectionReport::record).
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
    pub(super) fn escalates(self) -> bool {
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
