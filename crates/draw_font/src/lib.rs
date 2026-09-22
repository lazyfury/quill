//! `draw_font` — backend-neutral font loading, discovery, shaping and glyph
//! rasterization.
//!
//! Owns everything text-file-shaped so the render IR and the UI stay free of
//! font parsing:
//!
//! - **Discovery** ([`FontServer::families`]): scans the system font
//!   directories (or `QUILL_FONT`) into a list of families and weights, so an
//!   application can build a font picker.
//! - **Resolution** ([`FontServer::resolve`]): maps a `(family, weight)`
//!   request to a concrete face (file + face index), picking the nearest
//!   available weight and falling back to the default family.
//! - **Shaping** ([`FontServer::shape`]): bidi reordering plus `rustybuzz`
//!   shaping, with **per-character fallback** — a run is split by font coverage
//!   so CJK, Latin and other scripts can come from different faces in one line.
//! - **Rasterization** ([`FontServer::take_dirty_atlas`]): glyphs are
//!   rasterized on demand with `ab_glyph` into one shared shelf atlas, which a
//!   backend uploads to a texture.
//!
//! A backend (e.g. `draw_backend_wgpu`) holds a [`FontServer`] and turns the
//! returned [`GlyphSlot`]s into quads; it never parses a font itself.
//!
//! ```no_run
//! use draw_font::{FontConfig, FontRequest, FontServer};
//!
//! let server = FontServer::load_with(FontConfig::default());
//! for family in server.families() {
//!     println!("{} {:?}", family.name, family.weights);
//! }
//! let _id = server.resolve(&FontRequest::new("PingFang SC", 700));
//! ```

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_font";

mod atlas;
mod bitmap;
mod discovery;
mod face;
mod server;
mod shaping;

pub use face::GlyphSlot;
pub use server::{
    FaceRef, FontConfig, FontFamilyInfo, FontId, FontMetrics, FontMode, FontRequest, FontServer,
    PIXEL_GLYPH_RATIO,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_font");
    }
}
