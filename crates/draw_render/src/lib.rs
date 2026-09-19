//! `draw_render` — backend-neutral render IR.
//!
//! Owns `DrawCommand`, `DrawList`, `PaintContext`, resource handles and the
//! `RenderBackend` trait. Must not depend on any concrete backend or browser API.
//! Concrete types arrive in Stages 3-4.

/// Crate name, used by Stage 0 smoke tests.
pub const CRATE: &str = "draw_render";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_render");
    }
}
