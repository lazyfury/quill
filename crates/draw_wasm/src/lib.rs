//! `draw_wasm` — browser integration glue (events, RAF loop, canvas wiring).
//!
//! Allowed to depend on browser APIs. Kept out of the pure core so Scene/UI
//! stay headless-testable. Concrete implementation arrives in Stage 5.

/// Crate name, used by Stage 0 smoke tests.
pub const CRATE: &str = "draw_wasm";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_wasm");
    }
}
