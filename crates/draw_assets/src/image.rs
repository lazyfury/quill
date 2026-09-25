//! Decoded image data and the asset error type.

use std::fmt;

use draw_core::Size;

/// A decoded image, tightly packed as RGBA8 (straight alpha, row-major).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    width: u32,
    height: u32,
    rgba8: Vec<u8>,
}

impl DecodedImage {
    /// Builds an image from raw RGBA8 bytes after validating its shape.
    ///
    /// `rgba8` must hold exactly `width * height * 4` bytes; `width` and
    /// `height` must be non-zero.
    pub fn from_rgba8(width: u32, height: u32, rgba8: Vec<u8>) -> Result<Self, AssetError> {
        if width == 0 || height == 0 {
            return Err(AssetError::EmptyImage);
        }
        let expected = width as usize * height as usize * 4;
        if rgba8.len() != expected {
            return Err(AssetError::ByteLength {
                expected,
                actual: rgba8.len(),
            });
        }
        Ok(Self {
            width,
            height,
            rgba8,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The image size in logical pixels (a pixel is one logical unit).
    pub fn size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    /// The RGBA8 bytes, `width * height * 4` long.
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }

    /// Consumes the image and returns its RGBA8 bytes.
    pub fn into_rgba8(self) -> Vec<u8> {
        self.rgba8
    }
}

impl AsRef<[u8]> for DecodedImage {
    fn as_ref(&self) -> &[u8] {
        &self.rgba8
    }
}

/// Errors from decoding an asset.
#[derive(Debug)]
pub enum AssetError {
    /// The byte stream could not be decoded as PNG.
    Decode(png::DecodingError),
    /// A decoded buffer did not match `width * height * 4`.
    ByteLength { expected: usize, actual: usize },
    /// The image (or its decoded slice) had a zero width or height.
    EmptyImage,
    /// The decoded color type has no RGBA conversion.
    UnsupportedColorType(png::ColorType),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(f, "png decode failed: {error}"),
            Self::ByteLength { expected, actual } => {
                write!(f, "expected {expected} bytes, got {actual}")
            }
            Self::EmptyImage => f.write_str("image has a zero dimension"),
            Self::UnsupportedColorType(color) => {
                write!(f, "unsupported color type: {color:?}")
            }
        }
    }
}

impl std::error::Error for AssetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

impl From<png::DecodingError> for AssetError {
    fn from(error: png::DecodingError) -> Self {
        Self::Decode(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_rgba8_validates_the_shape() {
        assert!(matches!(
            DecodedImage::from_rgba8(0, 1, Vec::new()),
            Err(AssetError::EmptyImage)
        ));
        assert!(matches!(
            DecodedImage::from_rgba8(1, 1, vec![0; 3]),
            Err(AssetError::ByteLength {
                expected: 4,
                actual: 3
            })
        ));
        let image = DecodedImage::from_rgba8(2, 1, vec![0; 8]).unwrap();
        assert_eq!(image.size(), Size::new(2.0, 1.0));
        assert_eq!(image.rgba8().len(), 8);
        let _ = image.into_rgba8();
    }
}
