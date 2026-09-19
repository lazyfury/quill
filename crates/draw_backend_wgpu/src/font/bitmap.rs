//! Built-in fixed ASCII bitmap-font fallback.
//!
//! Used when no system/`QUILL_FONT` font can be loaded. `wgpu` has no text
//! stack, so ASCII is rasterized from the public-domain `font8x8` glyphs into
//! an `Rgba8Unorm` atlas. Each glyph cell is `8x8`; set pixels are white with
//! alpha 1, unset pixels fully transparent, so `texel * color` tints text.
//!
//! Characters outside printable ASCII (including CJK) sample a box-shaped
//! "missing glyph" cell and still advance by one `font_size`.

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

/// Atlas cell index of the "missing glyph" box (the spare cell after ASCII).
pub const MISSING_GLYPH_INDEX: u32 = LAST_CHAR - FIRST_CHAR + 1;

// The grid must have a cell for every printable ASCII glyph, plus one spare
// cell for the missing-glyph box.
const _: () = assert!(ATLAS_COLUMNS * ATLAS_ROWS > LAST_CHAR - FIRST_CHAR + 1);

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

    // A box in the spare cell for characters the atlas cannot render.
    let (cell_x, cell_y) = cell_origin_for_index(MISSING_GLYPH_INDEX);
    for step in 1..GLYPH_SIZE - 1 {
        for (x, y) in [
            (cell_x + step, cell_y + 1),
            (cell_x + step, cell_y + GLYPH_SIZE - 2),
            (cell_x + 1, cell_y + step),
            (cell_x + GLYPH_SIZE - 2, cell_y + step),
        ] {
            let offset = ((y * ATLAS_WIDTH + x) * 4) as usize;
            data[offset] = 255;
            data[offset + 1] = 255;
            data[offset + 2] = 255;
            data[offset + 3] = 255;
        }
    }
    data
}

/// Texture coordinates (`u0, v0, u1, v1`) for `ch`.
///
/// Characters outside printable ASCII map to the missing-glyph box.
pub fn glyph_uv(ch: char) -> [f32; 4] {
    let (cell_x, cell_y) = cell_origin_for_index(index_for(ch));
    let u0 = (cell_x as f32 + 0.5) / ATLAS_WIDTH as f32;
    let v0 = (cell_y as f32 + 0.5) / ATLAS_HEIGHT as f32;
    let u1 = (cell_x as f32 + GLYPH_SIZE as f32 - 0.5) / ATLAS_WIDTH as f32;
    let v1 = (cell_y as f32 + GLYPH_SIZE as f32 - 0.5) / ATLAS_HEIGHT as f32;
    [u0, v0, u1, v1]
}

fn index_for(ch: char) -> u32 {
    let code = ch as u32;
    if (FIRST_CHAR..=LAST_CHAR).contains(&code) {
        code - FIRST_CHAR
    } else {
        MISSING_GLYPH_INDEX
    }
}

fn cell_origin(ch: char) -> (u32, u32) {
    cell_origin_for_index(ch as u32 - FIRST_CHAR)
}

fn cell_origin_for_index(index: u32) -> (u32, u32) {
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
    fn unsupported_char_falls_back_to_missing_glyph_box() {
        let missing = glyph_uv(char::from_u32(0xFFFF).unwrap());
        assert_eq!(glyph_uv('中'), missing);
        assert_ne!(glyph_uv('?'), missing);
    }

    #[test]
    fn missing_glyph_cell_has_ink() {
        let atlas = build_atlas();
        let (cell_x, cell_y) = cell_origin_for_index(MISSING_GLYPH_INDEX);
        let offset = (((cell_y + 1) * ATLAS_WIDTH + cell_x + 1) * 4) as usize;
        assert_eq!(atlas[offset + 3], 255, "missing-glyph box should be opaque");
    }
}
