//! One loaded font face: parsing, coverage, shaping and glyph rasterization.
//!
//! Bytes are leaked once when a face is first used, because `ab_glyph::FontRef`
//! and `rustybuzz::Face` both borrow them for the `'static` lifetime of the
//! server. Only faces that are actually rendered (the primary plus any fallback
//! that covers a character) are loaded, so a discovery scan does not leak.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ab_glyph::{point, Font, FontRef, Glyph, GlyphId, PxScale, ScaleFont};
use draw_core::FontWeight;
use rustybuzz::UnicodeBuffer;

use crate::atlas::{Atlas, ATLAS_HEIGHT, ATLAS_WIDTH};
use crate::shaping::{direction, run_script};

/// A shaped glyph in font units (before scaling by `font_size / upem`).
#[derive(Clone, Copy)]
struct ShapedGlyph {
    glyph_id: u16,
    x_advance: f32,
    x_offset: f32,
    y_offset: f32,
}

/// Shaping cache capacity (entries).
const SHAPE_CACHE_CAP: usize = 4096;
/// Per-character advance cache capacity (entries).
const ADVANCE_CACHE_CAP: usize = 8192;

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

/// A font face loaded from disk with an on-demand glyph cache.
pub(crate) struct FontFace {
    family: String,
    weight: FontWeight,
    file: PathBuf,
    index: u32,
    /// Leaked once at load; borrowed by `font` and `rb`.
    #[allow(dead_code)]
    data: &'static [u8],
    font: FontRef<'static>,
    /// Shaper; derefs to `ttf_parser::Face` for coverage and names.
    rb: rustybuzz::Face<'static>,
    glyph_cache: RefCell<HashMap<(u32, u32), GlyphSlot>>,
    /// Shaped runs in font units, keyed by `(text, rtl)`.
    shape_cache: RefCell<HashMap<(Box<str>, bool), Rc<[ShapedGlyph]>>>,
    advance_cache: RefCell<HashMap<(u32, u32), f32>>,
}

impl FontFace {
    /// Reads and parses `file` at `index`.
    pub(crate) fn load(file: &Path, index: u32) -> Option<Self> {
        let bytes = std::fs::read(file).ok()?;
        let data: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let font = FontRef::try_from_slice_and_index(data, index).ok()?;
        let rb = rustybuzz::Face::from_slice(data, index)?;
        let family = crate::discovery::family_name(&rb).unwrap_or_else(|| {
            file.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let weight = FontWeight::new(rb.weight().to_number());
        Some(Self {
            family,
            weight,
            file: file.to_path_buf(),
            index,
            data,
            font,
            rb,
            glyph_cache: RefCell::new(HashMap::new()),
            shape_cache: RefCell::new(HashMap::new()),
            advance_cache: RefCell::new(HashMap::new()),
        })
    }

    #[allow(dead_code)]
    pub(crate) fn family(&self) -> &str {
        &self.family
    }

    #[allow(dead_code)]
    pub(crate) fn weight(&self) -> FontWeight {
        self.weight
    }

    #[allow(dead_code)]
    pub(crate) fn file(&self) -> &Path {
        &self.file
    }

    #[allow(dead_code)]
    pub(crate) fn index(&self) -> u32 {
        self.index
    }

    /// Whether the face's cmap has a glyph for `ch`.
    pub(crate) fn covers(&self, ch: char) -> bool {
        self.rb.glyph_index(ch).is_some()
    }

    /// Advance in device pixels (callers convert to logical).
    pub(crate) fn advance_px(&self, ch: char, px: f32) -> f32 {
        let key = (ch as u32, px.to_bits());
        if let Some(advance) = self.advance_cache.borrow().get(&key).copied() {
            return advance;
        }
        let scaled = self.font.as_scaled(px_scale(px));
        let advance = scaled.h_advance(self.font.glyph_id(ch));
        let mut cache = self.advance_cache.borrow_mut();
        if cache.len() >= ADVANCE_CACHE_CAP {
            cache.clear();
        }
        cache.insert(key, advance);
        advance
    }

    /// Shaped advance of `text` in device pixels.
    pub(crate) fn advance_run_px(&self, text: &str, rtl: bool, px: f32) -> f32 {
        let scale = self.unit_scale(px);
        self.shape(text, rtl)
            .iter()
            .map(|glyph| glyph.x_advance * scale)
            .sum()
    }

    /// Natural line height (ascent + descent + line gap) in device pixels.
    pub(crate) fn line_height_px(&self, px: f32) -> f32 {
        let scaled = self.font.as_scaled(px_scale(px));
        scaled.height() + scaled.line_gap()
    }

    pub(crate) fn ascent_px(&self, px: f32) -> f32 {
        let scaled = self.font.as_scaled(px_scale(px));
        scaled.ascent()
    }

    /// Atlas slot for `ch` at `px` device pixels, rasterizing it on first use.
    #[allow(dead_code)]
    pub(crate) fn glyph_px(&self, ch: char, px: f32, atlas: &RefCell<Atlas>) -> GlyphSlot {
        self.glyph_slot(self.font.glyph_id(ch).0, px, atlas)
    }

    /// Shapes `text` at `px` device pixels and returns one slot per glyph in
    /// visual order, rasterizing each glyph into `atlas`.
    pub(crate) fn shape_run_px(
        &self,
        text: &str,
        rtl: bool,
        px: f32,
        atlas: &RefCell<Atlas>,
    ) -> Vec<GlyphSlot> {
        let scale = self.unit_scale(px);
        self.shape(text, rtl)
            .iter()
            .map(|glyph| {
                let mut slot = self.glyph_slot(glyph.glyph_id, px, atlas);
                slot.advance = glyph.x_advance * scale;
                slot.offset[0] += glyph.x_offset * scale;
                slot.offset[1] += glyph.y_offset * scale;
                slot
            })
            .collect()
    }

    /// Device-pixels-per-font-unit factor, matching `ab_glyph`'s scaling of
    /// `h_advance_unscaled` (so shaped and unshaped advances agree).
    fn unit_scale(&self, px: f32) -> f32 {
        self.font.as_scaled(px_scale(px)).h_scale_factor()
    }

    /// Shapes `text` as a single run in `rtl` direction, in font units.
    ///
    /// Results are memoized by `(text, rtl)`; a drag/resize frame would
    /// otherwise re-run full HarfBuzz shaping for every text on screen.
    fn shape(&self, text: &str, rtl: bool) -> Rc<[ShapedGlyph]> {
        let key = (Box::<str>::from(text), rtl);
        if let Some(cached) = self.shape_cache.borrow().get(&key).cloned() {
            return cached;
        }
        let shaped: Rc<[ShapedGlyph]> = self.shape_uncached(text, rtl).into();
        let mut cache = self.shape_cache.borrow_mut();
        if cache.len() >= SHAPE_CACHE_CAP {
            cache.clear();
        }
        cache.insert(key, shaped.clone());
        shaped
    }

    fn shape_uncached(&self, text: &str, rtl: bool) -> Vec<ShapedGlyph> {
        let mut out = Vec::new();
        if text.is_empty() {
            return out;
        }
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(direction(rtl));
        buffer.set_script(run_script(text));

        let glyphs = rustybuzz::shape(&self.rb, &[], buffer);
        let infos = glyphs.glyph_infos();
        let positions = glyphs.glyph_positions();
        out.extend(infos.iter().zip(positions).map(|(info, pos)| ShapedGlyph {
            glyph_id: info.glyph_id as u16,
            x_advance: pos.x_advance as f32,
            x_offset: pos.x_offset as f32,
            y_offset: pos.y_offset as f32,
        }));
        out
    }

    /// Atlas slot for a glyph id at `px` device pixels, rasterizing on first use.
    fn glyph_slot(&self, glyph_id: u16, px: f32, atlas: &RefCell<Atlas>) -> GlyphSlot {
        let px = px.round().max(1.0);
        let key = (glyph_id as u32, px.to_bits());
        if let Some(slot) = self.glyph_cache.borrow().get(&key) {
            return *slot;
        }
        let slot = self.rasterize(glyph_id, px, atlas);
        self.glyph_cache.borrow_mut().insert(key, slot);
        slot
    }

    fn rasterize(&self, glyph_id: u16, px: f32, atlas: &RefCell<Atlas>) -> GlyphSlot {
        let scaled = self.font.as_scaled(px_scale(px));
        let id = GlyphId(glyph_id);
        let advance = scaled.h_advance(id);
        let glyph = Glyph {
            id,
            scale: px_scale(px),
            position: point(0.0, 0.0),
        };

        let Some(outline) = scaled.outline_glyph(glyph) else {
            // Whitespace / control: advance only.
            return GlyphSlot {
                advance,
                ..GlyphSlot::EMPTY
            };
        };
        let bounds = outline.px_bounds();
        let width = bounds.width().ceil().max(0.0) as u32;
        let height = bounds.height().ceil().max(0.0) as u32;
        if width == 0 || height == 0 {
            return GlyphSlot {
                advance,
                ..GlyphSlot::EMPTY
            };
        }

        let mut coverage = vec![0u8; (width * height) as usize];
        outline.draw(|x, y, value| {
            let index = (y * width + x) as usize;
            if index < coverage.len() {
                coverage[index] = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        });

        let mut atlas = atlas.borrow_mut();
        let Some((origin_x, origin_y)) = atlas.alloc(width, height) else {
            // Atlas full: keep the advance so layout stays correct.
            return GlyphSlot {
                advance,
                ..GlyphSlot::EMPTY
            };
        };
        atlas.blit(origin_x, origin_y, width, height, &coverage);

        let uv = [
            origin_x as f32 / ATLAS_WIDTH as f32,
            origin_y as f32 / ATLAS_HEIGHT as f32,
            (origin_x + width) as f32 / ATLAS_WIDTH as f32,
            (origin_y + height) as f32 / ATLAS_HEIGHT as f32,
        ];
        GlyphSlot {
            uv,
            size: [width as f32, height as f32],
            // `px_bounds` is already in y-down screen space relative to the
            // glyph position (baseline); its `min` is the top-left corner.
            offset: [bounds.min.x, bounds.min.y],
            advance,
        }
    }
}

fn px_scale(font_size: f32) -> PxScale {
    PxScale::from(font_size.max(1.0))
}
