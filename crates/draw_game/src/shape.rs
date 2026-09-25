//! Collision shapes and overlap queries.
//!
//! Shapes are local to a node (origin at the node origin, matching `Visual`),
//! and [`CollisionShape::world`] turns one into a world-axis [`WorldShape`] under
//! a node's [`draw_core::Transform2D`]. Rotation is folded into the world bounds
//! (an oriented box becomes its axis-aligned bounding box; a non-uniformly
//! scaled circle is approximated by its largest radius).

use draw_core::{Rect, Size, Transform2D, Vec2};

/// A collision shape in a node's local space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CollisionShape {
    /// An axis-aligned box from the node origin, `size` logical pixels.
    Aabb(Size),
    /// A circle centered at `offset` in local space.
    Circle { offset: Vec2, radius: f32 },
}

impl CollisionShape {
    /// An axis-aligned box of `size` from the node origin.
    pub fn aabb(size: Size) -> Self {
        Self::Aabb(size)
    }

    /// A circle of `radius` centered on the node origin.
    pub fn circle(radius: f32) -> Self {
        Self::Circle {
            offset: Vec2::ZERO,
            radius,
        }
    }

    /// A circle of `radius` centered at a local `offset`.
    pub fn circle_at(offset: Vec2, radius: f32) -> Self {
        Self::Circle { offset, radius }
    }

    /// The shape in world space under `transform`, as a world-axis bound.
    pub fn world(self, transform: Transform2D) -> WorldShape {
        match self {
            CollisionShape::Aabb(size) => {
                let half = Vec2::new(size.width * 0.5, size.height * 0.5);
                let center = transform.transform_point(half);
                let x = transform.transform_vector(Vec2::new(half.x, 0.0));
                let y = transform.transform_vector(Vec2::new(0.0, half.y));
                let half_x = x.x.abs() + y.x.abs();
                let half_y = x.y.abs() + y.y.abs();
                WorldShape::Aabb(Rect::from_center_size(
                    center,
                    Size::new(half_x * 2.0, half_y * 2.0),
                ))
            }
            CollisionShape::Circle { offset, radius } => {
                let center = transform.transform_point(offset);
                let x = transform.transform_vector(Vec2::new(radius, 0.0)).length();
                let y = transform.transform_vector(Vec2::new(0.0, radius)).length();
                WorldShape::Circle {
                    center,
                    radius: x.max(y),
                }
            }
        }
    }
}

/// A collision shape resolved to world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorldShape {
    Aabb(Rect),
    Circle { center: Vec2, radius: f32 },
}

impl WorldShape {
    /// Whether two world shapes overlap. Touching edges do not count (half-open
    /// rectangles).
    pub fn overlaps(self, other: Self) -> bool {
        match (self, other) {
            (WorldShape::Aabb(a), WorldShape::Aabb(b)) => a.intersects(b),
            (
                WorldShape::Circle {
                    center: a,
                    radius: ra,
                },
                WorldShape::Circle {
                    center: b,
                    radius: rb,
                },
            ) => circle_overlap(a, ra, b, rb),
            (WorldShape::Aabb(rect), WorldShape::Circle { center, radius })
            | (WorldShape::Circle { center, radius }, WorldShape::Aabb(rect)) => {
                aabb_circle_overlap(rect, center, radius)
            }
        }
    }
}

/// Whether two axis-aligned rectangles overlap.
pub fn aabb_overlap(a: Rect, b: Rect) -> bool {
    a.intersects(b)
}

/// Whether two circles overlap.
pub fn circle_overlap(a_center: Vec2, a_radius: f32, b_center: Vec2, b_radius: f32) -> bool {
    let reach = a_radius + b_radius;
    (a_center - b_center).length_squared() <= reach * reach
}

/// Whether a circle overlaps an axis-aligned rectangle.
pub fn aabb_circle_overlap(rect: Rect, center: Vec2, radius: f32) -> bool {
    let closest = Vec2::new(
        center.x.clamp(rect.left(), rect.right()),
        center.y.clamp(rect.top(), rect.bottom()),
    );
    (center - closest).length_squared() <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_overlap_respects_edges() {
        let a = Rect::from_min_size(Vec2::ZERO, Size::splat(10.0));
        let b = Rect::from_min_size(Vec2::new(9.0, 0.0), Size::splat(10.0));
        let apart = Rect::from_min_size(Vec2::new(10.0, 0.0), Size::splat(10.0));
        assert!(aabb_overlap(a, b));
        assert!(!aabb_overlap(a, apart), "touching edges do not overlap");
    }

    #[test]
    fn circle_and_aabb_queries() {
        assert!(circle_overlap(Vec2::ZERO, 5.0, Vec2::new(8.0, 0.0), 5.0));
        assert!(!circle_overlap(Vec2::ZERO, 5.0, Vec2::new(11.0, 0.0), 5.0));

        let rect = Rect::from_min_size(Vec2::ZERO, Size::splat(10.0));
        assert!(aabb_circle_overlap(rect, Vec2::new(12.0, 5.0), 3.0));
        assert!(!aabb_circle_overlap(rect, Vec2::new(14.0, 5.0), 3.0));
        assert!(aabb_circle_overlap(rect, Vec2::new(5.0, 5.0), 1.0));
    }

    #[test]
    fn world_shape_translates_and_scales() {
        let shape = CollisionShape::aabb(Size::new(10.0, 20.0));
        let moved = shape.world(Transform2D::from_translation(Vec2::new(100.0, 0.0)));
        assert_eq!(
            moved,
            WorldShape::Aabb(Rect::from_min_size(
                Vec2::new(100.0, 0.0),
                Size::new(10.0, 20.0)
            ))
        );

        let scaled = CollisionShape::circle(5.0).world(Transform2D::from_scale(Vec2::splat(2.0)));
        assert_eq!(
            scaled,
            WorldShape::Circle {
                center: Vec2::ZERO,
                radius: 10.0
            }
        );
    }

    #[test]
    fn a_rotated_box_becomes_its_axis_aligned_bound() {
        let square = CollisionShape::aabb(Size::splat(10.0));
        let rotated = square.world(Transform2D::from_rotation(std::f32::consts::FRAC_PI_4));
        let WorldShape::Aabb(rect) = rotated else {
            panic!("expected an AABB");
        };
        // A 10x10 square rotated 45 deg has a ~14.14 x 14.14 bounding box.
        assert!((rect.size.width - 14.142).abs() < 0.01);
    }
}
