//! `draw_bench_suite` — deterministic benchmarks for the quill CPU pipeline.
//!
//! This crate owns the *scenarios*; [`draw_bench`] owns the measurement. Keeping
//! them separate means the harness has no idea what a `SceneTree` or a `Ui` is,
//! and the suite has no timing code at all — it only builds fixtures and drives
//! one pipeline stage.
//!
//! Every fixture is built from a fixed size, contains no randomness and no I/O,
//! so the same scenario produces the same `DrawList` on every run (verified by
//! the tests). That is what makes a saved [`draw_bench::Baseline`] meaningful.
//!
//! Run it with:
//!
//! ```bash
//! cargo bench -p draw_bench_suite
//! cargo bench -p draw_bench_suite -- --filter scene/update
//! ```

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_bench_suite";

pub mod scenarios;
pub mod sink;

pub use sink::SinkBackend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_bench_suite");
    }
}
