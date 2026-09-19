//! `draw_backend_coregraphics` — macOS Core Graphics (Quartz 2D) render backend.
//!
//! Maps the backend-neutral [`draw_render::DrawList`] onto a `CGContext`, using
//! Core Text for text. Confined to `cfg(target_os = "macos")`; empty elsewhere.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_backend_coregraphics";

#[cfg(target_os = "macos")]
pub use coregraphics::{CoreGraphicsBackend, CoreGraphicsError};

#[cfg(target_os = "macos")]
mod coregraphics;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_backend_coregraphics");
    }
}
