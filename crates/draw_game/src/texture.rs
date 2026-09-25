//! Texture upload through the backend-neutral contract.

use draw_assets::DecodedImage;
use draw_render::{RenderBackend, TextureId};

/// Uploads a decoded image through the backend-neutral
/// [`RenderBackend::register_texture`] contract.
///
/// A backend without texture support still returns `Ok(())` (the trait's default
/// method), so this is always safe to call; the sprite simply does not display
/// there. The caller owns the [`TextureId`] and can reuse it for several
/// sprites.
///
/// ```ignore
/// let image = draw_assets::decode_png(&std::fs::read("player.png")?)?;
/// upload_texture(&mut backend, TextureId::new(1), &image)?;
/// ```
pub fn upload_texture<B: RenderBackend>(
    backend: &mut B,
    id: TextureId,
    image: &DecodedImage,
) -> Result<(), B::Error> {
    backend.register_texture(id, image.width(), image.height(), image.rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_backend_recording::RecordingBackend;

    #[test]
    fn upload_texture_reaches_the_backend_contract() {
        let image = DecodedImage::from_rgba8(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let mut backend = RecordingBackend::new();
        let id = TextureId::new(42);

        upload_texture(&mut backend, id, &image).unwrap();

        let registered = backend.texture(id).expect("metadata recorded");
        assert_eq!(registered.width, 2);
        assert_eq!(registered.height, 1);
    }
}
