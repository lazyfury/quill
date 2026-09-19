//! `draw_debug_ui` — a backend-neutral debug overlay for [`draw_profile`].
//!
//! Renders frame timing, structural counters and inspection findings as an
//! ordinary [`draw_ui`] panel. It owns its own [`Ui`](draw_ui::Ui) tree and is
//! painted after the application's UI, so it does not disturb the app's layout
//! or hit-testing.
//!
//! ```ignore
//! let mut overlay = DebugOverlay::new();
//!
//! // per frame, after painting the app into `ctx`:
//! overlay.update(&profiler, &report, viewport);
//! overlay.paint(&mut ctx);
//! ```
//!
//! Like every `draw_*` core crate this has no browser/backend/GPU dependency, so
//! the overlay is verified with native `cargo test`.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_debug_ui";

mod overlay;

pub use overlay::{Corner, DebugOverlay, OverlayConfig, OverlayText};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_debug_ui");
    }
}
