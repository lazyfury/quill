/// Four-sided inset/offset values, used for control offsets and margins.
///
/// Order matches CSS: `left`, `top`, `right`, `bottom`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Edges {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn all(v: f32) -> Self {
        Self::new(v, v, v, v)
    }

    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self::new(horizontal, vertical, horizontal, vertical)
    }

    pub const fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub const fn vertical(self) -> f32 {
        self.top + self.bottom
    }

    pub fn is_zero(self) -> bool {
        self.left == 0.0 && self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        let e = Edges::new(1.0, 2.0, 3.0, 4.0);
        assert!((e.horizontal() - 4.0).abs() < 1e-5);
        assert!((e.vertical() - 6.0).abs() < 1e-5);
        assert!(Edges::ZERO.is_zero());
        assert!(!Edges::all(1.0).is_zero());
        assert_eq!(Edges::symmetric(2.0, 3.0), Edges::new(2.0, 3.0, 2.0, 3.0));
    }
}
