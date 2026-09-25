//! `draw_game` — 2D game capabilities built on the scene tree.
//!
//! This crate is the Godot-style game layer: it depends on `draw_scene` (the
//! single `SceneTree`, `Node2D`, `Camera2D`, `CanvasLayer`) and never on a
//! backend or browser API. Sprites are ordinary `Node2D` nodes with a
//! [`draw_scene::Visual::Sprite`], so the existing paint/layer/camera pipeline
//! draws them unchanged.
//!
//! It is deliberately split from `draw_ui`: a game without a HUD never compiles
//! the UI crates (`draw_game` does **not** imply `ui`). The `quill` facade's
//! `game` feature forwards this crate.
//!
//! ```ignore
//! use draw_game::{upload_texture, Sprite};
//! use draw_render::TextureId;
//!
//! let png = std::fs::read("player.png")?;
//! let image = draw_assets::decode_png(&png)?;
//! let texture = TextureId::new(1);
//! upload_texture(&mut backend, texture, &image)?;
//!
//! let player = tree.add_child(tree.root(), Sprite::new(texture, image.size()));
//! ```
//!
//! Scope: sprites (region/atlas, flip, nine-slice) and their texture upload
//! helper. Sprite-frame animation, timers/signals and collision land in the
//! following sub-stages; rigid bodies and audio are out of scope.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_game";

mod sprite;
mod texture;

pub use sprite::Sprite;
pub use texture::upload_texture;

// Convenient handles a game needs from the core layers.
pub use draw_core::{NodeId, Rect, Size, Vec2};
pub use draw_render::TextureId;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_game");
    }
}
