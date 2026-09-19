//! `draw_wasm` — browser integration glue for the quill drawing core.
//!
//! Owns the canvas lookup, `requestAnimationFrame` loop, logical size / DPR
//! handling and the [`App`] hook. Kept separate from the pure core so Scene/UI
//! stay headless-testable.
//!
//! On non-`wasm32` targets this crate is intentionally empty.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_wasm";

#[cfg(target_arch = "wasm32")]
mod runner;

#[cfg(target_arch = "wasm32")]
pub use runner::{start, App};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_wasm");
    }
}
