//! Fonts for `DrawText`: configurable system/pixel fonts, metrics and glyph
//! rasterization.
//!
//! [`FontMode`] selects the look:
//!
//! - [`FontMode::System`] loads a real font with `ab_glyph` (from `QUILL_FONT`
//!   or a per-OS candidate list) and rasterizes glyphs on demand.
//! - [`FontMode::Pixel`] uses the built-in fixed `font8x8` bitmap (the original
//!   pixel look).
//!
//! [`FontConfig::device_pixel_rasterization`] makes system glyphs rasterize at
//! `font_size * scale`, so text is crisp on HiDPI displays; metrics stay in
//! logical pixels either way. If a system font cannot be loaded, `System` mode
//! falls back to the pixel bitmap automatically.
//!
//! [`Font`] is shared behind an [`Rc`] between the backend (which uploads the
//! atlas and tessellates glyphs) and [`FontMetrics`] handles (which hosts use to
//! build a `draw_ui::TextMeasurer`, keeping layout and rendering in sync).

mod bitmap;
mod system;

use std::cell::Cell;
use std::rc::Rc;

/// Which kind of font to use for `DrawText`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontMode {
    /// A real system font: proportional advances, CJK, antialiased.
    System,
    /// The built-in fixed-size pixel font (`font8x8`).
    Pixel,
}

/// Pixel-font glyph cell size as a fraction of `font_size`.
///
/// `font8x8` fills its full 8x8 cell (close to a whole em), so drawing it at
/// `font_size` makes glyphs look much larger than a proportional font at the
/// same size. Render the cell at this fraction (roughly a typical cap height),
/// rounded to a whole pixel for crisp output, while keeping the layout line box
/// at `font_size`.
pub const PIXEL_GLYPH_RATIO: f32 = 0.75;

/// Rounded glyph cell size in logical pixels for `font_size`.
fn pixel_cell(font_size: f32) -> f32 {
    (font_size * PIXEL_GLYPH_RATIO).round().max(1.0)
}

/// Font configuration for a [`WgpuBackend`](crate::WgpuBackend).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontConfig {
    pub mode: FontMode,
    /// Rasterize system glyphs at `font_size * scale` (crisp on HiDPI).
    ///
    /// Ignored in [`FontMode::Pixel`].
    pub device_pixel_rasterization: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            mode: FontMode::System,
            device_pixel_rasterization: true,
        }
    }
}

/// A rasterized glyph's atlas placement and layout metrics.
///
/// `size`/`offset`/`advance` are in **logical** pixels, so callers never deal
/// with the rasterization scale. `offset` is relative to the pen (baseline,
/// left) with y down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphSlot {
    pub uv: [f32; 4],
    pub size: [f32; 2],
    pub offset: [f32; 2],
    pub advance: f32,
}

impl GlyphSlot {
    pub const EMPTY: Self = Self {
        uv: [0.0; 4],
        size: [0.0; 2],
        offset: [0.0; 2],
        advance: 0.0,
    };
}

enum Engine {
    System(Box<system::SystemFont>),
    Bitmap,
}

/// A loaded font (or the bitmap fallback) plus its glyph atlas.
pub struct Font {
    engine: Engine,
    name: Option<String>,
    scale: Cell<f32>,
    device_pixels: Cell<bool>,
}

impl Font {
    /// Loads a font using the default [`FontConfig`].
    pub fn load() -> Self {
        Self::load_with(FontConfig::default())
    }

    /// Loads the font described by `config`.
    pub fn load_with(config: FontConfig) -> Self {
        let (engine, name) = match config.mode {
            FontMode::Pixel => (Engine::Bitmap, None),
            FontMode::System => match system::SystemFont::load() {
                Some(font) => {
                    let name = font.name().to_string();
                    (Engine::System(Box::new(font)), Some(name))
                }
                None => (Engine::Bitmap, None),
            },
        };
        Self {
            engine,
            name,
            scale: Cell::new(1.0),
            device_pixels: Cell::new(config.device_pixel_rasterization),
        }
    }

    /// The built-in fixed pixel bitmap font.
    pub fn bitmap() -> Self {
        Self {
            engine: Engine::Bitmap,
            name: None,
            scale: Cell::new(1.0),
            device_pixels: Cell::new(false),
        }
    }

    /// Whether a real system font was loaded.
    pub fn is_system(&self) -> bool {
        matches!(self.engine, Engine::System(_))
    }

    /// The mode this font resolves to (`System` may have fallen back to
    /// `Pixel`).
    pub fn mode(&self) -> FontMode {
        if self.is_system() {
            FontMode::System
        } else {
            FontMode::Pixel
        }
    }

    /// Font source path, when a system font was loaded.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn set_scale(&self, scale: f32) {
        self.scale.set(scale.max(0.0));
    }

    pub fn set_device_pixel_rasterization(&self, enabled: bool) {
        self.device_pixels.set(enabled);
    }

    /// Rasterization `(pixels, divisor)`: convert logical size to atlas pixels
    /// and back.
    fn raster_params(&self, font_size: f32) -> (f32, f32) {
        match &self.engine {
            Engine::System(_) if self.device_pixels.get() => {
                let scale = self.scale.get().max(1e-3);
                ((font_size * scale).round().max(1.0), scale)
            }
            Engine::System(_) => (font_size.round().max(1.0), 1.0),
            Engine::Bitmap => (font_size, 1.0),
        }
    }

    pub fn advance(&self, ch: char, font_size: f32) -> f32 {
        match &self.engine {
            Engine::System(font) => {
                let (px, divisor) = self.raster_params(font_size);
                font.advance_px(ch, px) / divisor
            }
            Engine::Bitmap => pixel_cell(font_size),
        }
    }

    pub fn line_height(&self, font_size: f32) -> f32 {
        match &self.engine {
            Engine::System(font) => {
                let (px, divisor) = self.raster_params(font_size);
                font.line_height_px(px) / divisor
            }
            Engine::Bitmap => font_size,
        }
    }

    pub fn ascent(&self, font_size: f32) -> f32 {
        match &self.engine {
            Engine::System(font) => {
                let (px, divisor) = self.raster_params(font_size);
                font.ascent_px(px) / divisor
            }
            // Keep the (smaller) pixel cell centred in the `font_size` line box.
            Engine::Bitmap => font_size * 0.5 + pixel_cell(font_size) * 0.5,
        }
    }

    /// Atlas slot for `ch` (logical units), rasterizing on first use.
    pub fn glyph(&self, ch: char, font_size: f32) -> GlyphSlot {
        match &self.engine {
            Engine::System(font) => {
                let (px, divisor) = self.raster_params(font_size);
                let mut slot = font.glyph_px(ch, px);
                slot.advance /= divisor;
                slot.size = [slot.size[0] / divisor, slot.size[1] / divisor];
                slot.offset = [slot.offset[0] / divisor, slot.offset[1] / divisor];
                slot
            }
            Engine::Bitmap => {
                let cell = pixel_cell(font_size);
                GlyphSlot {
                    uv: bitmap::glyph_uv(ch),
                    size: [cell, cell],
                    offset: [0.0, -cell],
                    advance: cell,
                }
            }
        }
    }

    /// Shapes `text` at `font_size` into glyphs in visual order (logical
    /// units). System fonts apply kerning/ligatures and bidi reordering; the
    /// bitmap font maps one slot per character.
    pub fn shape(&self, text: &str, font_size: f32) -> Vec<GlyphSlot> {
        match &self.engine {
            Engine::System(font) => {
                let (px, divisor) = self.raster_params(font_size);
                font.shape_px(text, px)
                    .into_iter()
                    .map(|mut slot| {
                        slot.advance /= divisor;
                        slot.size = [slot.size[0] / divisor, slot.size[1] / divisor];
                        slot.offset = [slot.offset[0] / divisor, slot.offset[1] / divisor];
                        slot
                    })
                    .collect()
            }
            Engine::Bitmap => text.chars().map(|ch| self.glyph(ch, font_size)).collect(),
        }
    }

    /// Shaped advance width of `text` (logical units).
    pub fn advance_run(&self, text: &str, font_size: f32) -> f32 {
        match &self.engine {
            Engine::System(font) => {
                let (px, divisor) = self.raster_params(font_size);
                font.advance_run_px(text, px) / divisor
            }
            Engine::Bitmap => text.chars().count() as f32 * pixel_cell(font_size),
        }
    }

    /// Full atlas pixels if new glyphs were rasterized since the last call.
    pub fn take_dirty_atlas(&self) -> Option<Vec<u8>> {
        match &self.engine {
            Engine::System(font) => font.take_dirty_atlas(),
            Engine::Bitmap => None,
        }
    }

    /// `(width, height)` of the atlas texture this font needs.
    pub fn atlas_size(&self) -> (u32, u32) {
        match &self.engine {
            Engine::System(_) => (system::ATLAS_WIDTH, system::ATLAS_HEIGHT),
            Engine::Bitmap => (bitmap::ATLAS_WIDTH, bitmap::ATLAS_HEIGHT),
        }
    }

    /// Initial atlas contents (system fonts start empty).
    pub fn initial_atlas(&self) -> Vec<u8> {
        match &self.engine {
            Engine::System(_) => {
                vec![0u8; (system::ATLAS_WIDTH * system::ATLAS_HEIGHT * 4) as usize]
            }
            Engine::Bitmap => bitmap::build_atlas(),
        }
    }
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Font")
            .field("mode", &self.mode())
            .field("name", &self.name)
            .finish()
    }
}

/// A cloneable handle to the backend's font metrics.
///
/// Hosts wrap this in a `draw_ui::TextMeasurer` so layout measures text with
/// the exact metrics the backend renders with.
#[derive(Clone)]
pub struct FontMetrics {
    font: Rc<Font>,
}

impl FontMetrics {
    pub fn new(font: Rc<Font>) -> Self {
        Self { font }
    }

    pub fn advance(&self, ch: char, font_size: f32) -> f32 {
        self.font.advance(ch, font_size)
    }

    pub fn line_height(&self, font_size: f32) -> f32 {
        self.font.line_height(font_size)
    }

    pub fn ascent(&self, font_size: f32) -> f32 {
        self.font.ascent(font_size)
    }

    /// Shaped advance width of `text`, matching what the backend draws.
    pub fn measure_run(&self, text: &str, font_size: f32) -> f32 {
        self.font.advance_run(text, font_size)
    }

    pub fn is_system(&self) -> bool {
        self.font.is_system()
    }

    pub fn name(&self) -> Option<&str> {
        self.font.name()
    }
}

impl std::fmt::Debug for FontMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontMetrics")
            .field("system", &self.is_system())
            .field("name", &self.name())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_font_has_fixed_advance_and_covers_ascii() {
        let font = Font::bitmap();
        assert!(!font.is_system());
        assert_eq!(font.mode(), FontMode::Pixel);
        let cell = pixel_cell(16.0);
        assert_eq!(font.advance('i', 16.0), cell);
        assert_eq!(font.advance('W', 16.0), cell);
        assert_eq!(font.line_height(16.0), 16.0);
        // CJK falls back to the missing-glyph box but still advances.
        assert_eq!(font.glyph('中', 16.0).advance, cell);
    }

    #[test]
    fn pixel_glyph_is_smaller_than_the_em_and_centred() {
        let font = Font::bitmap();
        let slot = font.glyph('A', 16.0);
        let line_height = font.line_height(16.0);
        let ascent = font.ascent(16.0);
        // The 8x8 cell would otherwise fill the whole em and look oversized.
        assert!(slot.size[1] < line_height);
        // Equal space above and below keeps labels vertically centred.
        let above = ascent - slot.size[1];
        let below = line_height - ascent;
        assert!((above - below).abs() < 1e-4, "above={above} below={below}");
    }

    #[test]
    fn pixel_mode_never_loads_a_system_font() {
        let font = Font::load_with(FontConfig {
            mode: FontMode::Pixel,
            device_pixel_rasterization: true,
        });
        assert_eq!(font.mode(), FontMode::Pixel);
        assert!(!font.is_system());
    }

    #[test]
    fn bitmap_atlas_is_uploaded_once() {
        let font = Font::bitmap();
        assert_eq!(
            font.atlas_size(),
            (bitmap::ATLAS_WIDTH, bitmap::ATLAS_HEIGHT)
        );
        assert_eq!(
            font.initial_atlas().len() as u32,
            bitmap::ATLAS_WIDTH * bitmap::ATLAS_HEIGHT * 4
        );
        assert!(font.take_dirty_atlas().is_none());
    }

    #[test]
    fn system_font_metrics_are_proportional_when_available() {
        let Some(font) = system::SystemFont::load() else {
            return; // no system font in this environment
        };
        // 'i' is narrower than 'W' for essentially every proportional font.
        assert!(
            font.advance_px('i', 32.0) < font.advance_px('W', 32.0),
            "advances were not proportional"
        );
        assert!(font.line_height_px(32.0) >= 32.0);
        assert!(font.ascent_px(32.0) > 0.0);
    }

    #[test]
    fn system_font_rasterizes_ascii_and_cjk() {
        let Some(font) = system::SystemFont::load() else {
            return;
        };
        assert!(font.glyph_px('A', 24.0).size[0] > 0.0);
        assert!(font.glyph_px('A', 24.0).size[1] > 0.0);
        let cjk = font.glyph_px('中', 24.0);
        assert!(cjk.advance > 0.0);
        if cjk.size[0] > 0.0 {
            assert!(cjk.size[1] > 0.0);
        }
        assert!(font.take_dirty_atlas().is_some());
    }

    #[test]
    fn device_pixel_rasterization_keeps_metrics_logical() {
        let Some(_) = system::SystemFont::load() else {
            return;
        };
        let font = Font::load_with(FontConfig::default());
        assert!(font.is_system());

        font.set_scale(1.0);
        let advance_1x = font.advance('M', 16.0);
        let size_1x = font.glyph('M', 16.0).size;

        font.set_scale(2.0);
        let advance_2x = font.advance('M', 16.0);
        let size_2x = font.glyph('M', 16.0).size;

        // Logical metrics are scale-invariant (up to rounding).
        assert!(
            (advance_1x - advance_2x).abs() < 0.51,
            "advance changed with scale: {advance_1x} vs {advance_2x}"
        );
        assert!(
            (size_1x[1] - size_2x[1]).abs() <= 1.01,
            "glyph height changed with scale: {size_1x:?} vs {size_2x:?}"
        );
    }

    #[test]
    fn logical_rasterization_is_independent_of_scale() {
        let Some(_) = system::SystemFont::load() else {
            return;
        };
        let font = Font::load_with(FontConfig {
            mode: FontMode::System,
            device_pixel_rasterization: false,
        });
        font.set_scale(1.0);
        let advance_1x = font.advance('M', 16.0);
        font.set_scale(2.0);
        let advance_2x = font.advance('M', 16.0);
        assert!((advance_1x - advance_2x).abs() < 1e-4);
    }

    #[test]
    fn shaped_advance_matches_shaped_glyph_slots() {
        let font = Font::load_with(FontConfig::default());
        if !font.is_system() {
            return; // bitmap font has no shaping
        }
        let text = "AVfi";
        let total: f32 = font.shape(text, 24.0).iter().map(|slot| slot.advance).sum();
        assert!(
            (font.advance_run(text, 24.0) - total).abs() < 1e-3,
            "advance_run must match the shaped glyph slots"
        );
    }

    #[test]
    fn shaping_does_not_widen_a_latin_run() {
        let Some(font) = system::SystemFont::load() else {
            return;
        };
        // Kerning may tighten a pair but must never add width here; ligatures
        // may replace glyphs but keep the run covered.
        let pair = font.advance_run_px("AV", 64.0);
        let separate = font.advance_px('A', 64.0) + font.advance_px('V', 64.0);
        assert!(
            pair <= separate + 1.0,
            "shaping widened \"AV\": {pair} > {separate}"
        );

        let slots = font.shape_px("ffi", 64.0);
        assert!(!slots.is_empty());
        let run: f32 = slots.iter().map(|slot| slot.advance).sum();
        assert!((run - font.advance_run_px("ffi", 64.0)).abs() < 1e-3);
    }
}
