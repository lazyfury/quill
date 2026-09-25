//! `quill` — the application facade over the fine-grained core crates.
//!
//! The core stays deliberately split so its dependency boundaries stay
//! enforceable. Applications depend on this one crate with opt-in features
//! instead of listing every crate:
//!
//! | feature | adds |
//! |---|---|
//! | `ui` | `draw_core`, `draw_render`, `draw_scene`, `draw_theme`, `draw_ui`, `draw_components` |
//! | `anim` | `draw_anim` (+ the `draw_core` / `draw_scene` it targets) |
//! | `game` | `draw_game` + `draw_assets` (+ `draw_core` / `draw_render` / `draw_scene`) |
//!
//! Disabled crates are not compiled at all. A UI-only app enables `ui` and a
//! backend; it never enables `game` or `anim`. `game` does **not** imply `ui`,
//! and `anim` is independent of both. This crate contains no logic — only
//! re-exports.
//!
//! ```toml
//! # UI app
//! quill = { path = ".../quill", default-features = false, features = ["ui"] }
//! # 2D game
//! quill = { path = ".../quill", default-features = false, features = ["game"] }
//! ```

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "quill";

#[cfg(any(feature = "ui", feature = "anim", feature = "game"))]
pub use draw_core;

#[cfg(any(feature = "ui", feature = "anim", feature = "game"))]
pub use draw_scene;

#[cfg(any(feature = "ui", feature = "game"))]
pub use draw_render;

#[cfg(feature = "ui")]
pub use draw_theme;

#[cfg(feature = "ui")]
pub use draw_ui;

#[cfg(feature = "ui")]
pub use draw_components;

#[cfg(feature = "anim")]
pub use draw_anim;

#[cfg(feature = "game")]
pub use draw_game;

#[cfg(feature = "game")]
pub use draw_assets;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "quill");
    }
}
