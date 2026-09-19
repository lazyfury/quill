//! `draw_debug_ui` — backend-neutral debug visuals for quill.
//!
//! Two independent overlays, both drawn as ordinary backend-neutral
//! `DrawCommand`s (no browser/backend/GPU dependency, verified with native
//! `cargo test`):
//!
//! - [`DebugOverlay`] — **component debug drawing**: a yellow border around
//!   every visible `Control` plus a `Name #id` label in its top-left corner. It
//!   wraps [`Ui::paint_debug`](draw_ui::Ui::paint_debug) and draws over the
//!   application's own UI.
//! - [`PerformanceOverlay`] — the frame-timing / inspection panel fed by
//!   [`draw_profile`]; it owns its own `Ui` tree and is painted after the app UI.
//!
//! ```ignore
//! let mut debug = DebugOverlay::new();          // component bounds
//! let mut perf = PerformanceOverlay::new();      // profiler panel
//!
//! // per frame, after painting the app UI into `ctx`:
//! debug.paint(&app_ui, &mut ctx);
//! perf.update(&profiler, &report, viewport);
//! perf.paint(&mut ctx);
//! ```

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_debug_ui";

mod component;
mod performance;

pub use component::DebugOverlay;
pub use performance::{Corner, OverlayConfig, OverlayText, PerformanceOverlay};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_debug_ui");
    }
}
