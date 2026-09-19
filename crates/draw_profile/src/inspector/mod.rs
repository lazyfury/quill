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
//!
//! The types live in [`finding`] / [`report`] and the checks in [`audit`].

mod audit;
mod finding;
mod report;

pub use audit::{inspect, inspect_draw_list, inspect_frame, inspect_with};
pub use finding::{Finding, FindingCode, Severity};
pub use report::{InspectionConfig, InspectionReport};

#[cfg(test)]
mod tests;
