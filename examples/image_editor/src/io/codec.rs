//! PNG 编解码：`PixelBuffer <-> PNG 字节`。不碰文件系统。
//!
//! 只处理 8 位色深：编码固定写 RGBA8；解码把调色板 / 灰度 / 16 位统一
//! `normalize_to_color8` 成 8 位，再展开成 RGBA。这样导入任意常见 PNG 都不会
//! 因“不支持的颜色类型”失败。

use crate::document::PixelBuffer;

use super::IoError;

/// 把一块 RGBA 像素编码成 PNG 字节（8 位 RGBA，带 alpha）。
pub fn encode_png(pixels: &PixelBuffer) -> Result<Vec<u8>, IoError> {
    if pixels.width == 0 || pixels.height == 0 {
        return Err(IoError::Invalid("空图像无法编码".to_string()));
    }
    let expected = (pixels.width as usize) * (pixels.height as usize) * 4;
    if pixels.data.len() < expected {
        return Err(IoError::Invalid(format!(
            "像素数据不足：需要 {expected} 字节，只有 {}",
            pixels.data.len()
        )));
    }

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, pixels.width, pixels.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(IoError::Encode)?;
        writer
            .write_image_data(&pixels.data[..expected])
            .map_err(IoError::Encode)?;
        writer.finish().map_err(IoError::Encode)?;
    }
    Ok(out)
}

/// 把 PNG 字节解码成一块 RGBA 像素。
pub fn decode_png(bytes: &[u8]) -> Result<PixelBuffer, IoError> {
    let mut decoder = png::Decoder::new(bytes);
    // 调色板 -> RGB、16 位 -> 8 位；alpha / 灰度通道数仍由 `color_type` 决定。
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(IoError::Decode)?;

    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).map_err(IoError::Decode)?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(IoError::Invalid(format!(
            "只支持 8 位色深，收到 {:?}",
            info.bit_depth
        )));
    }

    let channels = channel_count(info.color_type)
        .ok_or_else(|| IoError::Invalid(format!("不支持的颜色类型 {:?}", info.color_type)))?;
    let (width, height) = (info.width, info.height);
    if width == 0 || height == 0 {
        return Err(IoError::Invalid("空图像".to_string()));
    }

    let row = info.line_size;
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for y in 0..height as usize {
        let start = y * row;
        let pixels = &buffer[start..start + row];
        for x in 0..width as usize {
            let source = &pixels[x * channels..x * channels + channels];
            match source {
                [r, g, b, a] => rgba.extend_from_slice(&[*r, *g, *b, *a]),
                [r, g, b] => rgba.extend_from_slice(&[*r, *g, *b, 255]),
                [gray] => rgba.extend_from_slice(&[*gray, *gray, *gray, 255]),
                [gray, a] => rgba.extend_from_slice(&[*gray, *gray, *gray, *a]),
                _ => unreachable!("channel_count 与切片长度一致"),
            }
        }
    }

    PixelBuffer::from_rgba8(width, height, rgba)
        .ok_or_else(|| IoError::Invalid("解码后的尺寸与字节数不匹配".to_string()))
}

/// 每种颜色类型的通道数。
fn channel_count(color_type: png::ColorType) -> Option<usize> {
    match color_type {
        png::ColorType::Grayscale => Some(1),
        png::ColorType::GrayscaleAlpha => Some(2),
        png::ColorType::Rgb => Some(3),
        png::ColorType::Rgba => Some(4),
        // `normalize_to_color8` 已经把调色板展开成 RGB。
        png::ColorType::Indexed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Color;

    #[test]
    fn a_buffer_round_trips_through_png() {
        let mut pixels = PixelBuffer::new(3, 2);
        pixels.set_pixel(0, 0, Color::new(10, 20, 30, 40));
        pixels.set_pixel(2, 1, Color::RED);

        let bytes = encode_png(&pixels).expect("encode");
        let decoded = decode_png(&bytes).expect("decode");

        assert_eq!((decoded.width, decoded.height), (3, 2));
        assert_eq!(decoded.get_pixel(0, 0), Color::new(10, 20, 30, 40));
        assert_eq!(decoded.get_pixel(2, 1), Color::RED);
        assert_eq!(decoded.get_pixel(1, 0), Color::TRANSPARENT);
    }

    #[test]
    fn decoding_garbage_is_an_error() {
        assert!(matches!(
            decode_png(b"not a png at all"),
            Err(IoError::Decode(_))
        ));
    }

    #[test]
    fn an_empty_buffer_refuses_to_encode() {
        let error = encode_png(&PixelBuffer::new(0, 0)).unwrap_err();
        assert!(matches!(error, IoError::Invalid(_)));
    }

    #[test]
    fn a_png_without_an_alpha_channel_decodes_opaque() {
        // 手写一个 1x1 的 RGB PNG，验证 3 通道展开成 a=255。
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[1, 2, 3]).unwrap();
            writer.finish().unwrap();
        }
        let decoded = decode_png(&out).expect("decode rgb");
        assert_eq!(decoded.get_pixel(0, 0), Color::new(1, 2, 3, 255));
    }
}
