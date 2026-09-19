use crate::size::Size;
use crate::vec2::Vec2;

/// An axis-aligned rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub origin: Vec2,
    pub size: Size,
}

impl Rect {
    pub const ZERO: Self = Self {
        origin: Vec2::ZERO,
        size: Size::ZERO,
    };

    pub const fn new(origin: Vec2, size: Size) -> Self {
        Self { origin, size }
    }

    pub const fn from_min_size(min: Vec2, size: Size) -> Self {
        Self { origin: min, size }
    }

    pub fn from_min_max(min: Vec2, max: Vec2) -> Self {
        Self::from_min_size(min, Size::new(max.x - min.x, max.y - min.y))
    }

    pub fn from_center_size(center: Vec2, size: Size) -> Self {
        Self::from_min_size(
            Vec2::new(center.x - size.width * 0.5, center.y - size.height * 0.5),
            size,
        )
    }

    pub fn left(self) -> f32 {
        self.origin.x
    }

    pub fn top(self) -> f32 {
        self.origin.y
    }

    pub fn right(self) -> f32 {
        self.origin.x + self.size.width
    }

    pub fn bottom(self) -> f32 {
        self.origin.y + self.size.height
    }

    pub fn min(self) -> Vec2 {
        self.origin
    }

    pub fn max(self) -> Vec2 {
        Vec2::new(self.right(), self.bottom())
    }

    pub fn center(self) -> Vec2 {
        Vec2::new(
            self.origin.x + self.size.width * 0.5,
            self.origin.y + self.size.height * 0.5,
        )
    }

    pub fn is_empty(self) -> bool {
        self.size.is_empty()
    }

    /// Point membership with a half-open interval: `[left, right) x [top, bottom)`.
    ///
    /// Half-open avoids double-counting points shared by adjacent rectangles.
    pub fn contains(self, point: Vec2) -> bool {
        point.x >= self.left()
            && point.x < self.right()
            && point.y >= self.top()
            && point.y < self.bottom()
    }

    pub fn contains_rect(self, other: Rect) -> bool {
        other.left() >= self.left()
            && other.right() <= self.right()
            && other.top() >= self.top()
            && other.bottom() <= self.bottom()
    }

    pub fn intersects(self, other: Rect) -> bool {
        self.left() < other.right()
            && other.left() < self.right()
            && self.top() < other.bottom()
            && other.top() < self.bottom()
    }

    pub fn intersection(self, other: Rect) -> Option<Rect> {
        let min = self.min().max(other.min());
        let max = self.max().min(other.max());
        if min.x < max.x && min.y < max.y {
            Some(Rect::from_min_max(min, max))
        } else {
            None
        }
    }

    pub fn union(self, other: Rect) -> Rect {
        let min = self.min().min(other.min());
        let max = self.max().max(other.max());
        Rect::from_min_max(min, max)
    }

    pub fn translate(self, offset: Vec2) -> Rect {
        Rect::new(self.origin + offset, self.size)
    }

    /// Expands the rectangle by the given edges.
    pub fn inflate(self, edges: crate::edges::Edges) -> Rect {
        Rect::new(
            Vec2::new(self.origin.x - edges.left, self.origin.y - edges.top),
            Size::new(
                self.size.width + edges.horizontal(),
                self.size.height + edges.vertical(),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges::Edges;

    #[test]
    fn rect_contains() {
        let r = Rect::from_min_size(Vec2::new(10.0, 20.0), Size::new(100.0, 50.0));
        assert!(r.contains(Vec2::new(10.0, 20.0))); // min inclusive
        assert!(r.contains(Vec2::new(50.0, 40.0)));
        assert!(!r.contains(Vec2::new(110.0, 40.0))); // max exclusive
        assert!(!r.contains(Vec2::new(9.0, 20.0)));
        assert!(!r.contains(Vec2::new(10.0, 70.0)));

        assert_eq!(r.left(), 10.0);
        assert_eq!(r.top(), 20.0);
        assert_eq!(r.right(), 110.0);
        assert_eq!(r.bottom(), 70.0);
        assert_eq!(r.center(), Vec2::new(60.0, 45.0));
    }

    #[test]
    fn rect_intersection_and_union() {
        let a = Rect::from_min_size(Vec2::ZERO, Size::new(10.0, 10.0));
        let b = Rect::from_min_size(Vec2::new(5.0, 5.0), Size::new(10.0, 10.0));
        assert!(a.intersects(b));
        assert_eq!(
            a.intersection(b),
            Some(Rect::from_min_size(
                Vec2::new(5.0, 5.0),
                Size::new(5.0, 5.0)
            ))
        );
        assert_eq!(
            a.union(b),
            Rect::from_min_size(Vec2::ZERO, Size::new(15.0, 15.0))
        );

        let c = Rect::from_min_size(Vec2::new(20.0, 20.0), Size::new(5.0, 5.0));
        assert!(!a.intersects(c));
        assert_eq!(a.intersection(c), None);
    }

    #[test]
    fn rect_translate_and_inflate() {
        let r = Rect::from_min_size(Vec2::new(10.0, 10.0), Size::new(10.0, 10.0));
        assert_eq!(
            r.translate(Vec2::new(5.0, -5.0)),
            Rect::from_min_size(Vec2::new(15.0, 5.0), Size::new(10.0, 10.0))
        );
        assert_eq!(
            r.inflate(Edges::all(2.0)),
            Rect::from_min_size(Vec2::new(8.0, 8.0), Size::new(14.0, 14.0))
        );
    }

    #[test]
    fn from_center_size() {
        let r = Rect::from_center_size(Vec2::new(50.0, 50.0), Size::new(10.0, 20.0));
        assert_eq!(r.origin, Vec2::new(45.0, 40.0));
        assert_eq!(r.center(), Vec2::new(50.0, 50.0));
    }
}
