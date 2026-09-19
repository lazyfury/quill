//! `draw_backend_canvas` — HTML Canvas 2D render backend.
//!
//! This is the only place (besides `draw_wasm` and the web demo) allowed to
//! touch browser APIs. It consumes `draw_render` IR only.
//! Concrete implementation arrives in Stage 5.

/// Crate name, used by Stage 0 smoke tests.
pub const CRATE: &str = "draw_backend_canvas";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_backend_canvas");
    }
}
