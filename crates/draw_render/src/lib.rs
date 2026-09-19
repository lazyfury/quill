//! `draw_render` — the backend-neutral render intermediate representation (IR).
//!
//! Owns [`DrawCommand`], [`DrawList`], [`PaintContext`], [`Paint`] and resource
//! handles ([`TextureId`]). It depends only on `draw_core` and **must never**
//! reference a concrete backend, browser API, or GPU object, so any backend can
//! consume the same IR.
//!
//! # Pipeline position
//!
//! ```text
//! Scene / UI --paint--> PaintContext --> DrawList --> RenderBackend --> Pixels
//! ```
//!
//! The [`RenderBackend`] trait defines the frame lifecycle a backend implements.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_render";

mod backend;
mod command;
mod list;
mod texture;

pub use backend::RenderBackend;
pub use command::{DrawCommand, Paint, TextAlign};
pub use list::{DrawList, PaintContext};
pub use texture::TextureId;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_render");
    }
}
