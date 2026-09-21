//! 文档空间里的整数像素坐标。

/// 相对文档原点的像素坐标（左上为原点，y 向下）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_the_origin() {
        assert_eq!(Point::ZERO, Point::new(0, 0));
        assert_eq!(Point::new(-3, 7).x, -3);
    }
}
