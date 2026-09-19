//! `draw_profile` — backend-neutral performance inspection for the quill pipeline.
//!
//! This crate observes the pipeline stages without owning any of them:
//!
//! ```text
//! Input -> SceneTree -> Update -> Layout -> Paint -> DrawList -> RenderBackend -> Pixels
//!                        ^^^^^^^^^^^^^^^^^^^^^^^  ^^^^^^^^^^   ^^^^^^^^^^^^
//!                        \_____ draw_profile observes these __/
//! ```
//!
//! It depends only on `draw_core` and `draw_render` and **must never** reference a
//! concrete backend, browser API, or GPU object. Everything here is plain data
//! plus pure functions, so all of it is testable with native `cargo test` and can
//! be rendered by any of the existing backends.
//!
//! # Pieces
//!
//! - [`Profiler`] collects a bounded history of [`FrameStats`] and derives a
//!   [`FrameSummary`] (averages, min/max, FPS).
//! - [`Phase`] / [`StageTimes`] hold the per-stage timing breakdown.
//! - [`FrameCounters`] hold cheap structural counts (nodes, controls, commands).
//! - [`inspect`] audits a frame's `DrawList` + counters and returns an
//!   [`InspectionReport`] of severity-ranked [`Finding`]s.
//!
//! Timing is recorded as explicit milliseconds; the crate never calls
//! `Instant::now()` itself. The host (demo / WASM runner) measures with
//! [`std::time::Instant`] and feeds the numbers in. That keeps the model
//! deterministic and the tests exact.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_profile";

mod inspector;
mod phase;
mod profiler;
mod stats;

pub use inspector::{
    inspect, inspect_draw_list, inspect_frame, inspect_with, Finding, FindingCode,
    InspectionConfig, InspectionReport, Severity,
};
pub use phase::Phase;
pub use profiler::{FrameSummary, Profiler, DEFAULT_CAPACITY};
pub use stats::{FrameCounters, FrameStats, StageTimes};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_profile");
    }
}
