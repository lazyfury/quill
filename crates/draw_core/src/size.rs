/// A width/height pair in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub const fn splat(v: f32) -> Self {
        Self {
            width: v,
            height: v,
        }
    }

    pub fn area(self) -> f32 {
        self.width * self.height
    }

    /// True when either dimension is non-positive.
    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    pub fn is_finite(self) -> bool {
        self.width.is_finite() && self.height.is_finite()
    }

    pub fn min(self, other: Self) -> Self {
        Self::new(self.width.min(other.width), self.height.min(other.height))
    }

    pub fn max(self, other: Self) -> Self {
        Self::new(self.width.max(other.width), self.height.max(other.height))
    }

    pub fn to_array(self) -> [f32; 2] {
        [self.width, self.height]
    }

    pub fn from_array(v: [f32; 2]) -> Self {
        Self::new(v[0], v[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        let s = Size::new(3.0, 4.0);
        assert!((s.area() - 12.0).abs() < 1e-5);
        assert!(!s.is_empty());
        assert!(Size::new(0.0, 4.0).is_empty());
        assert!(Size::new(-1.0, 4.0).is_empty());
        assert_eq!(s.max(Size::new(1.0, 8.0)), Size::new(3.0, 8.0));
    }
}
