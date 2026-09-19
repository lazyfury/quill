//! `draw_backend_wgpu` — a `wgpu` render backend.
//!
//! Consumes the backend-neutral [`draw_render::DrawList`] and rasterizes it with
//! `wgpu`. Geometry, opacity and clip are resolved on the CPU; the GPU pass is a
//! single textured-triangle pipeline, so solid shapes, images and bitmap text
//! all share one pipeline.
//!
//! The default target is an offscreen `Rgba8Unorm` texture: the same `DrawList`
//! that the Canvas backend draws in a browser can be rendered and **read back as
//! pixels** under native `cargo test`, with no window and no screenshot. To draw
//! into a window instead, hand a surface texture view to
//! [`WgpuBackend::begin_frame_with_view`] and present it after `end_frame` (see
//! `examples/wgpu_demo`).
//!
//! `wgpu` never enters `draw_core` / `draw_scene` / `draw_ui` / `draw_render`;
//! it is confined to this crate.
//!
//! ```no_run
//! use draw_backend_wgpu::WgpuBackend;
//! use draw_core::{Color, Rect, Size, Vec2, ViewportSize};
//! use draw_render::{PaintContext, RenderBackend};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut backend = WgpuBackend::new()?;
//! let mut ctx = PaintContext::new();
//! ctx.fill_rect(
//!     Rect::from_min_size(Vec2::ZERO, Size::splat(16.0)),
//!     Color::RED,
//! );
//! let viewport = ViewportSize::new(Size::new(32.0, 32.0));
//! backend.begin_frame(viewport)?;
//! backend.submit(&ctx.into_draw_list())?;
//! backend.end_frame()?;
//! let pixels = backend.read_pixels()?;
//! assert_eq!(pixels.pixel(8, 8), Some([255, 0, 0, 255]));
//! # Ok(())
//! # }
//! ```

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_backend_wgpu";

// WGPU is a native backend. On `wasm32` the crate is intentionally empty, like
// `draw_backend_canvas` is on native, so the workspace still checks for the web
// targets.
#[cfg(not(target_arch = "wasm32"))]
mod backend;
#[cfg(not(target_arch = "wasm32"))]
mod font;
#[cfg(not(target_arch = "wasm32"))]
mod shader;

#[cfg(not(target_arch = "wasm32"))]
pub use backend::{PixelBuffer, WgpuBackend, WgpuError};
#[cfg(not(target_arch = "wasm32"))]
pub use font::{FontConfig, FontMetrics, FontMode, PIXEL_GLYPH_RATIO};

/// Re-export of the `wgpu` version this backend is built against, so callers
/// (e.g. window runners) can create surfaces, adapters and device resources
/// without risking a version mismatch.
#[cfg(not(target_arch = "wasm32"))]
pub use wgpu;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_backend_wgpu");
    }
}
