//! Font service re-export.
//!
//! Font loading, discovery, shaping and rasterization live in the
//! backend-neutral [`draw_font`] crate; this module keeps the backend's
//! historical names (`Font`, `FontConfig`, ...) pointing at it.

pub use draw_font::FontServer as Font;
pub use draw_font::{
    FontConfig, FontFamilyInfo, FontId, FontMetrics, FontMode, FontRequest, FontServer,
    PIXEL_GLYPH_RATIO,
};
