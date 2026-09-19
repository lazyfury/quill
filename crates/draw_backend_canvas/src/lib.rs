//! `draw_backend_canvas` — HTML Canvas 2D render backend.
//!
//! Maps the backend-neutral [`draw_render::DrawList`] onto the Canvas 2D API.
//! Browser dependencies (`web-sys`) are confined to this crate (and `draw_wasm`),
//! which is the only place a `CanvasRenderingContext2d` may appear.
//!
//! On non-`wasm32` targets this crate is intentionally empty so the workspace
//! still builds and tests natively.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_backend_canvas";

#[cfg(target_arch = "wasm32")]
mod canvas;

#[cfg(target_arch = "wasm32")]
pub use canvas::{font_spec, Canvas2dBackend, CanvasError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_backend_canvas");
    }
}
