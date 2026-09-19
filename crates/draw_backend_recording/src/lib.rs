//! `draw_backend_recording` — headless backend that records a `DrawList`.
//!
//! Used for deterministic tests without a browser. Concrete implementation
//! arrives in Stage 4.

/// Crate name, used by Stage 0 smoke tests.
pub const CRATE: &str = "draw_backend_recording";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_backend_recording");
    }
}
