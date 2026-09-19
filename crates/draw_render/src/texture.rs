/// A backend-neutral handle to a texture resource.
///
/// The core never stores a backend texture object; backends map this handle to
/// their own resource (an `ImageBitmap` for Canvas, a GPU texture for WGPU, ...).
/// [`TextureId::INVALID`] denotes "no texture".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(u32);

impl TextureId {
    /// Sentinel meaning "no texture".
    pub const INVALID: Self = Self(u32::MAX);

    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn is_valid(self) -> bool {
        self.0 != u32::MAX
    }
}

impl Default for TextureId {
    fn default() -> Self {
        Self::INVALID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validity() {
        assert!(!TextureId::default().is_valid());
        assert!(!TextureId::INVALID.is_valid());
        let id = TextureId::new(7);
        assert!(id.is_valid());
        assert_eq!(id.raw(), 7);
    }
}
