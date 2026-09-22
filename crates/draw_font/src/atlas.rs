//! A shelf-packing RGBA8 glyph atlas shared by every loaded face.
//!
//! Glyphs are white RGB with coverage alpha, so `texel * color` tints text.
//! One atlas holds the glyphs of all faces (regular, bold, fallback scripts),
//! so a backend still uploads a single texture.

/// Dynamic atlas width in texels.
pub(crate) const ATLAS_WIDTH: u32 = 1024;
/// Dynamic atlas height in texels.
pub(crate) const ATLAS_HEIGHT: u32 = 1024;
/// Gap between packed glyphs, to avoid bilinear bleed.
const PADDING: u32 = 1;

/// A simple shelf-packing RGBA8 atlas (white RGB + coverage alpha).
pub(crate) struct Atlas {
    pixels: Vec<u8>,
    pen_x: u32,
    pen_y: u32,
    row_height: u32,
    dirty: bool,
}

impl Atlas {
    pub(crate) fn new() -> Self {
        Self {
            pixels: vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize],
            pen_x: 0,
            pen_y: 0,
            row_height: 0,
            dirty: false,
        }
    }

    /// The full atlas pixels if new glyphs were rasterized since the last call.
    pub(crate) fn take_dirty(&mut self) -> Option<Vec<u8>> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        Some(self.pixels.clone())
    }

    pub(crate) fn alloc(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
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

    pub(crate) fn blit(&mut self, x: u32, y: u32, width: u32, height: u32, coverage: &[u8]) {
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
