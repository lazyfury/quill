//! `draw_core` — foundation layer (math, color, IDs, base types).
//!
//! Must not depend on any other `draw_*` crate, nor on browser/WASM APIs.
//! Concrete types arrive in Stage 1.

/// Crate name, used by Stage 0 smoke tests.
pub const CRATE: &str = "draw_core";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_core");
    }
}
