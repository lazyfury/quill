use crate::texture::TextureId;

/// A backend-neutral handle to an offscreen render target.
///
/// A render target is a texture that a backend renders a `DrawList` into (via
/// [`RenderBackend::render_to_target`](crate::RenderBackend::render_to_target))
/// and that subsequent frames can sample with
/// [`DrawImage`](crate::DrawCommand::DrawImage). It therefore **shares the id
/// space with [`TextureId`](crate::TextureId)**: [`RenderTargetId::texture`] is
/// the handle to pass to `DrawImage`. Use distinct numbers from uploaded
/// textures so the two never collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenderTargetId(TextureId);

impl RenderTargetId {
    /// Sentinel meaning "no render target".
    pub const INVALID: Self = Self(TextureId::INVALID);

    /// Wraps an existing [`TextureId`].
    pub const fn new(texture: TextureId) -> Self {
        Self(texture)
    }

    /// Wraps a raw id.
    pub const fn from_raw(raw: u32) -> Self {
        Self(TextureId::new(raw))
    }

    /// The texture handle to sample this target with.
    pub const fn texture(self) -> TextureId {
        self.0
    }

    /// The raw id.
    pub const fn raw(self) -> u32 {
        self.0.raw()
    }

    pub const fn is_valid(self) -> bool {
        self.0.is_valid()
    }
}

impl From<TextureId> for RenderTargetId {
    fn from(texture: TextureId) -> Self {
        Self(texture)
    }
}

impl Default for RenderTargetId {
    fn default() -> Self {
        Self::INVALID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_the_texture_id_space() {
        let target = RenderTargetId::from_raw(9);
        assert!(target.is_valid());
        assert_eq!(target.texture(), TextureId::new(9));
        assert_eq!(target.raw(), 9);
        assert!(!RenderTargetId::default().is_valid());
    }
}
