//! `draw_core` — foundation layer for the quill drawing core.
//!
//! This crate owns the backend-neutral base types every other crate builds on:
//! math ([`Vec2`], [`Size`], [`Rect`], [`Edges`], [`Transform2D`]), [`Color`],
//! stable handles ([`NodeId`]) and the logical [`ViewportSize`] model.
//!
//! It has no dependencies outside the Rust standard library and **must never**
//! depend on a renderer or browser API.
//!
//! # Coordinate convention
//!
//! - Origin is the **top-left** of the viewport.
//! - `+X` points right, `+Y` points **down**.
//! - Units are **logical pixels**. Device pixels (and browser DPR) are a backend
//!   concern and never enter core business logic.
//! - Rotations are in **radians**, positive from `+X` toward `+Y` (so in the
//!   y-down viewport a positive angle appears clockwise on screen).
//! - Rectangles are axis-aligned, defined by an `origin` and a `size`.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_core";

mod color;
mod cursor;
mod edges;
mod id;
mod input;
mod rect;
mod size;
mod transform;
mod vec2;
mod viewport_size;

pub use color::Color;
pub use cursor::Cursor;
pub use edges::Edges;
pub use id::{NodeId, NodeIdAllocator};
pub use input::{EventResult, InputEvent, InputState, Key, PointerButton};
pub use rect::Rect;
pub use size::Size;
pub use transform::Transform2D;
pub use vec2::Vec2;
pub use viewport_size::ViewportSize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_core");
    }
}
