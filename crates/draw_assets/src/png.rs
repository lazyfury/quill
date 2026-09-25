//! PNG decoding into RGBA8.

use std::io::Cursor;

use crate::image::{AssetError, DecodedImage};

/// Decodes a PNG byte stream into a [`DecodedImage`] (tightly packed RGBA8).
///
/// The decoder normalizes bit depth to 8 and expands palette / low-bit images,
/// then converts any non-alpha color type to opaque RGBA, so callers always get
/// the same layout regardless of the source PNG's encoding.
pub fn decode_png(bytes: &[u8]) -> Result<DecodedImage, AssetError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;

    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer)?;
    let data = &buffer[..info.buffer_size()];

    let rgba8 =
        to_rgba8(info.color_type, data).ok_or(AssetError::UnsupportedColorType(info.color_type))?;
    DecodedImage::from_rgba8(info.width, info.height, rgba8)
}

/// Converts a decoded 8-bit channel buffer to RGBA8.
fn to_rgba8(color_type: png::ColorType, data: &[u8]) -> Option<Vec<u8>> {
    use png::ColorType;
    Some(match color_type {
        ColorType::Rgba => data.to_vec(),
        ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        ColorType::Grayscale => data
            .iter()
            .flat_map(|&gray| [gray, gray, gray, 255])
            .collect(),
        ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        // `EXPAND` turns indexed images into RGB/RGBA before this point.
        ColorType::Indexed => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real 2x2 RGBA PNG: row 0 red, green; row 1 blue, white.
    const FIXTURE: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72,
        0xb6, 0x0d, 0x24, 0x00, 0x00, 0x00, 0x12, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xf8,
        0xcf, 0xc0, 0xf0, 0x1f, 0x0c, 0x81, 0x34, 0x18, 0x00, 0x00, 0x49, 0xc8, 0x09, 0xf7, 0x03,
        0xd9, 0x64, 0xf1, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn encode(color: png::ColorType, width: u32, height: u32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(color);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(data).unwrap();
        }
        out
    }

    #[test]
    fn decodes_a_real_png_to_rgba8() {
        let image = decode_png(FIXTURE).unwrap();
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        assert_eq!(
            image.rgba8(),
            &[
                255, 0, 0, 255, 0, 255, 0, 255, // row 0
                0, 0, 255, 255, 255, 255, 255, 255, // row 1
            ]
        );
    }

    #[test]
    fn rgb_input_gains_an_opaque_alpha_channel() {
        let bytes = encode(png::ColorType::Rgb, 1, 1, &[10, 20, 30]);
        let image = decode_png(&bytes).unwrap();
        assert_eq!(image.rgba8(), &[10, 20, 30, 255]);
    }

    #[test]
    fn a_garbage_stream_is_rejected() {
        assert!(matches!(
            decode_png(&[0x00, 0x01, 0x02, 0x03]),
            Err(AssetError::Decode(_))
        ));
    }
}
