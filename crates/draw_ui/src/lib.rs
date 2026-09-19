//! `draw_ui` — controls, layout, containers and basic UI behavior.
//!
//! May depend on `draw_core` and `draw_scene`. Must not depend on render backends.
//! Concrete types arrive in Stage 6.

/// Crate name, used by Stage 0 smoke tests.
pub const CRATE: &str = "draw_ui";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_ui");
    }
}
