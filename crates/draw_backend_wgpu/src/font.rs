//! A compact built-in bitmap-font atlas for [`DrawCommand::DrawText`].
//!
//! WGPU has no text stack, so the backend rasterizes ASCII with the public
//! domain `font8x8` glyphs into an `Rgba8Unorm` atlas at startup. Each glyph
//! cell is `8x8`; set pixels are white (RGB 1,1,1) with alpha 1, unset pixels
//! are fully transparent, so the fragment shader (`texel * color`) tints text
//! with the command's paint color.
//!
//! This is deliberately simple (fixed-width ASCII); real font shaping belongs
//! in a font-rendering layer, not in a backend.
//!
//! [`DrawCommand::DrawText`]: draw_render::DrawCommand::DrawText

use font8x8::UnicodeFonts;

/// Glyph cell size in texels.
pub const GLYPH_SIZE: u32 = 8;
/// Atlas grid columns.
pub const ATLAS_COLUMNS: u32 = 16;
/// Atlas grid rows (16 * 6 = 96 cells >= 95 printable ASCII glyphs).
pub const ATLAS_ROWS: u32 = 6;
/// Atlas width in texels.
pub const ATLAS_WIDTH: u32 = ATLAS_COLUMNS * GLYPH_SIZE;
/// Atlas height in texels.
pub const ATLAS_HEIGHT: u32 = ATLAS_ROWS * GLYPH_SIZE;

/// First character covered by the atlas (`' '`).
pub const FIRST_CHAR: u32 = 0x20;
/// Last character covered by the atlas (`'~'`).
pub const LAST_CHAR: u32 = 0x7e;

// The grid must have a cell for every printable ASCII glyph.
const _: () = assert!(ATLAS_COLUMNS * ATLAS_ROWS > LAST_CHAR - FIRST_CHAR);

/// Rasterizes the printable ASCII range into a tightly packed RGBA8 atlas.
pub fn build_atlas() -> Vec<u8> {
    let mut data = vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize];
    for code in FIRST_CHAR..=LAST_CHAR {
        let ch = char::from_u32(code).unwrap_or('?');
        let Some(glyph) = font8x8::BASIC_FONTS.get(ch) else {
            continue;
        };
        let (cell_x, cell_y) = cell_origin(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..GLYPH_SIZE as usize {
                // font8x8 stores each row with the leftmost pixel in bit 0.
                if (bits >> col) & 1 == 0 {
                    continue;
                }
                let x = cell_x + col as u32;
                let y = cell_y + row as u32;
                let offset = ((y * ATLAS_WIDTH + x) * 4) as usize;
                data[offset] = 255;
                data[offset + 1] = 255;
                data[offset + 2] = 255;
                data[offset + 3] = 255;
            }
        }
    }
    data
}

/// Texture coordinates (`u0, v0, u1, v1`) for `ch`, or `'?'` for characters
/// outside the atlas.
///
/// UVs are inset by half a texel so nearest-neighbour sampling never bleeds
/// into a neighbouring glyph cell.
pub fn glyph_uv(ch: char) -> [f32; 4] {
    let code = ch as u32;
    let ch = if (FIRST_CHAR..=LAST_CHAR).contains(&code) {
        ch
    } else {
        '?'
    };
    let (cell_x, cell_y) = cell_origin(ch);
    let u0 = (cell_x as f32 + 0.5) / ATLAS_WIDTH as f32;
    let v0 = (cell_y as f32 + 0.5) / ATLAS_HEIGHT as f32;
    let u1 = (cell_x as f32 + GLYPH_SIZE as f32 - 0.5) / ATLAS_WIDTH as f32;
    let v1 = (cell_y as f32 + GLYPH_SIZE as f32 - 0.5) / ATLAS_HEIGHT as f32;
    [u0, v0, u1, v1]
}

fn cell_origin(ch: char) -> (u32, u32) {
    let index = ch as u32 - FIRST_CHAR;
    (
        (index % ATLAS_COLUMNS) * GLYPH_SIZE,
        (index / ATLAS_COLUMNS) * GLYPH_SIZE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_has_expected_dimensions() {
        assert_eq!(
            build_atlas().len(),
            (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize
        );
    }

    #[test]
    fn glyph_uv_is_inside_atlas_and_increasing() {
        let uv = glyph_uv('A');
        assert!(uv[0] < uv[2] && uv[1] < uv[3]);
        assert!(uv[0] >= 0.0 && uv[2] <= 1.0);
        assert!(uv[1] >= 0.0 && uv[3] <= 1.0);
    }

    #[test]
    fn unsupported_char_falls_back_to_question_mark() {
        assert_eq!(glyph_uv('中'), glyph_uv('?'));
    }
}
