//! `draw_backend_recording` — a headless [`RenderBackend`] for tests.
//!
//! Records each frame's viewport and concatenated [`DrawList`] commands so the
//! full `Scene -> DrawList -> RenderBackend` pipeline can be verified with
//! native `cargo test`, no browser required.
//!
//! [`DrawList`]: draw_render::DrawList

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_backend_recording";

mod assert;
mod backend;

pub use assert::CommandAsserts;
pub use backend::{RecordedFrame, RecordingBackend, RecordingError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_backend_recording");
    }
}
