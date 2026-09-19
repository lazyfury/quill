//! `draw_scene` — scene tree: `Node`, `SceneTree`, `CanvasItem`, `Node2D`.
//!
//! May depend on `draw_core`. Must not depend on `draw_render` or any browser API.
//! Concrete types arrive in Stage 2.

/// Crate name, used by Stage 0 smoke tests.
pub const CRATE: &str = "draw_scene";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_scene");
    }
}
