//! System font loading (via `ab_glyph`) and a lazily-populated glyph atlas.
//!
//! A font is located from `QUILL_FONT` (explicit path) or a short per-OS
//! candidate list. Glyphs are rasterized on demand at `font_size` and packed
//! into a shelf atlas; the caller uploads [`SystemFont::take_dirty_atlas`]
//! after processing a frame's commands.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};

use super::GlyphSlot;

/// Dynamic atlas width in texels.
pub const ATLAS_WIDTH: u32 = 1024;
/// Dynamic atlas height in texels.
pub const ATLAS_HEIGHT: u32 = 1024;
/// Gap between packed glyphs, to avoid bilinear bleed.
const PADDING: u32 = 1;

/// A font loaded from disk with an on-demand glyph atlas.
pub struct SystemFont {
    name: String,
    font: FontVec,
    atlas: RefCell<Atlas>,
    cache: RefCell<HashMap<(u32, u32), GlyphSlot>>,
}

impl SystemFont {
    /// Searches `QUILL_FONT` and per-OS candidates for a loadable font.
    pub fn load() -> Option<Self> {
        for path in candidate_paths() {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            if let Ok(font) = FontVec::try_from_vec_and_index(bytes, 0) {
                return Some(Self::new(font, path.display().to_string()));
            }
        }
        None
    }

    pub fn new(font: FontVec, name: String) -> Self {
        Self {
            name,
            font,
            atlas: RefCell::new(Atlas::new()),
            cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Advance in device pixels (callers convert to logical).
    pub fn advance_px(&self, ch: char, px: f32) -> f32 {
        let scaled = self.font.as_scaled(px_scale(px));
        scaled.h_advance(scaled.glyph_id(ch))
    }

    /// Natural line height (ascent + descent + line gap) in device pixels.
    pub fn line_height_px(&self, px: f32) -> f32 {
        let scaled = self.font.as_scaled(px_scale(px));
        scaled.height() + scaled.line_gap()
    }

    pub fn ascent_px(&self, px: f32) -> f32 {
        let scaled = self.font.as_scaled(px_scale(px));
        scaled.ascent()
    }

    /// Atlas slot for `ch` at `px` device pixels, rasterizing it on first use.
    pub fn glyph_px(&self, ch: char, px: f32) -> GlyphSlot {
        let px = px.round().max(1.0);
        let key = (ch as u32, px.to_bits());
        if let Some(slot) = self.cache.borrow().get(&key) {
            return *slot;
        }
        let slot = self.rasterize(ch, px);
        self.cache.borrow_mut().insert(key, slot);
        slot
    }

    /// Returns the full atlas pixels if new glyphs were rasterized since the
    /// last call.
    pub fn take_dirty_atlas(&self) -> Option<Vec<u8>> {
        let mut atlas = self.atlas.borrow_mut();
        if !atlas.dirty {
            return None;
        }
        atlas.dirty = false;
        Some(atlas.pixels.clone())
    }

    fn rasterize(&self, ch: char, px: f32) -> GlyphSlot {
        let scaled = self.font.as_scaled(px_scale(px));
        let glyph = scaled.scaled_glyph(ch);
        let advance = scaled.h_advance(glyph.id);

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

        let mut atlas = self.atlas.borrow_mut();
        let Some((origin_x, origin_y)) = atlas.alloc(width, height) else {
            // Atlas full: keep the advance so layout stays correct.
            return GlyphSlot {
                advance,
                ..GlyphSlot::EMPTY
            };
        };
        atlas.blit(origin_x, origin_y, width, height, &coverage);

        let uv = [
            (origin_x as f32 + 0.5) / ATLAS_WIDTH as f32,
            (origin_y as f32 + 0.5) / ATLAS_HEIGHT as f32,
            ((origin_x + width) as f32 - 0.5) / ATLAS_WIDTH as f32,
            ((origin_y + height) as f32 - 0.5) / ATLAS_HEIGHT as f32,
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

impl std::fmt::Debug for SystemFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemFont")
            .field("name", &self.name)
            .finish()
    }
}

fn px_scale(font_size: f32) -> PxScale {
    PxScale::from(font_size.max(1.0))
}

/// `QUILL_FONT` override plus a short per-OS candidate list.
fn candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::var("QUILL_FONT") {
        candidates.push(PathBuf::from(path));
    }

    #[cfg(target_os = "macos")]
    {
        candidates.push("/System/Library/Fonts/Supplemental/Arial Unicode.ttf".into());
        candidates.push("/Library/Fonts/Arial Unicode.ttf".into());
        candidates.push("/System/Library/Fonts/Supplemental/Arial.ttf".into());
        candidates.push("/System/Library/Fonts/Helvetica.ttc".into());
    }
    #[cfg(target_os = "linux")]
    {
        candidates.push("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into());
        candidates.push("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf".into());
        candidates.push("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".into());
    }
    #[cfg(target_os = "windows")]
    {
        candidates.push("C:/Windows/Fonts/arial.ttf".into());
        candidates.push("C:/Windows/Fonts/msyh.ttc".into());
    }
    candidates
}

/// A simple shelf-packing RGBA8 atlas (white RGB + coverage alpha).
struct Atlas {
    pixels: Vec<u8>,
    pen_x: u32,
    pen_y: u32,
    row_height: u32,
    dirty: bool,
}

impl Atlas {
    fn new() -> Self {
        Self {
            pixels: vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize],
            pen_x: 0,
            pen_y: 0,
            row_height: 0,
            dirty: false,
        }
    }

    fn alloc(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 {
            return None;
        }
        if self.pen_x + width + PADDING > ATLAS_WIDTH {
            self.pen_x = 0;
            self.pen_y += self.row_height + PADDING;
            self.row_height = 0;
        }
        if self.pen_y + height + PADDING > ATLAS_HEIGHT {
            return None;
        }
        let origin = (self.pen_x, self.pen_y);
        self.pen_x += width + PADDING;
        self.row_height = self.row_height.max(height);
        Some(origin)
    }

    fn blit(&mut self, x: u32, y: u32, width: u32, height: u32, coverage: &[u8]) {
        for row in 0..height {
            for col in 0..width {
                let alpha = coverage[(row * width + col) as usize];
                if alpha == 0 {
                    continue;
                }
                let index = (((y + row) * ATLAS_WIDTH + (x + col)) * 4) as usize;
                self.pixels[index] = 255;
                self.pixels[index + 1] = 255;
                self.pixels[index + 2] = 255;
                self.pixels[index + 3] = alpha;
            }
        }
        self.dirty = true;
    }
}
