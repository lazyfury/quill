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
//!
//! Disabled crates are not compiled at all. A UI-only app enables `ui` and a
//! backend; it never enables `anim` (and, from Stage 31, never `game`). This
//! crate contains no logic — only re-exports.
//!
//! ```toml
//! quill = { path = ".../quill", default-features = false, features = ["ui", "anim"] }
//! ```

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "quill";

#[cfg(any(feature = "ui", feature = "anim"))]
pub use draw_core;

#[cfg(any(feature = "ui", feature = "anim"))]
pub use draw_scene;

#[cfg(feature = "ui")]
pub use draw_render;

#[cfg(feature = "ui")]
pub use draw_theme;

#[cfg(feature = "ui")]
pub use draw_ui;

#[cfg(feature = "ui")]
pub use draw_components;

#[cfg(feature = "anim")]
pub use draw_anim;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "quill");
    }
}
