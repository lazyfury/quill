use crate::rect::Rect;
use crate::size::Size;

/// The logical drawing area of a render target.
///
/// The viewport stores **logical pixels only**. Device pixels and browser DPR
/// are a backend concern: given a scale factor, the backend derives the backing
/// store size via [`Viewport::device_size`] without that factor ever entering
/// core logic.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Viewport {
    logical_size: Size,
}

impl Viewport {
    pub const fn new(logical_size: Size) -> Self {
        Self { logical_size }
    }

    pub const fn logical_size(self) -> Size {
        self.logical_size
    }

    pub fn set_logical_size(&mut self, size: Size) {
        self.logical_size = size;
    }

    /// The full viewport as a rectangle starting at the origin.
    pub fn logical_rect(self) -> Rect {
        Rect::from_min_size(crate::vec2::Vec2::ZERO, self.logical_size)
    }

    /// Backing-store size in device pixels for the given scale factor (DPR).
    ///
    /// This is a pure conversion helper; the core stores no DPR.
    pub fn device_size(self, scale_factor: f32) -> Size {
        Size::new(
            self.logical_size.width * scale_factor,
            self.logical_size.height * scale_factor,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vec2::Vec2;

    #[test]
    fn viewport_logical_size() {
        let vp = Viewport::new(Size::new(800.0, 600.0));
        assert_eq!(vp.logical_size(), Size::new(800.0, 600.0));
        assert_eq!(vp.logical_rect().max(), Vec2::new(800.0, 600.0));
        assert!(vp.logical_rect().contains(Vec2::new(799.0, 599.0)));
        assert!(!vp.logical_rect().contains(Vec2::new(800.0, 600.0)));
    }

    #[test]
    fn device_size_uses_scale_factor() {
        let vp = Viewport::new(Size::new(400.0, 300.0));
        assert_eq!(vp.device_size(2.0), Size::new(800.0, 600.0));
        assert_eq!(vp.device_size(1.0), Size::new(400.0, 300.0));
    }

    #[test]
    fn set_logical_size() {
        let mut vp = Viewport::default();
        assert_eq!(vp.logical_size(), Size::ZERO);
        vp.set_logical_size(Size::new(10.0, 20.0));
        assert_eq!(vp.logical_size(), Size::new(10.0, 20.0));
    }
}
