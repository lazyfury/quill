use std::ops::Mul;

use crate::vec2::Vec2;

/// A 2D affine transform stored as two basis axes plus an origin
/// (a 2x3 matrix, column-major).
///
/// A point `p` maps to `x_axis * p.x + y_axis * p.y + origin`.
/// A direction vector ignores `origin`.
///
/// # Conventions
///
/// - `+X` right, `+Y` down (y-down viewport).
/// - Positive rotation is from `+X` toward `+Y` (visually clockwise on screen).
/// - `a * b` applies `b` first, then `a` (so `T * R * S` reads left-to-right).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub x_axis: Vec2,
    pub y_axis: Vec2,
    pub origin: Vec2,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform2D {
    pub const IDENTITY: Self = Self {
        x_axis: Vec2::X,
        y_axis: Vec2::Y,
        origin: Vec2::ZERO,
    };

    pub const fn new(x_axis: Vec2, y_axis: Vec2, origin: Vec2) -> Self {
        Self {
            x_axis,
            y_axis,
            origin,
        }
    }

    pub fn from_translation(translation: Vec2) -> Self {
        Self {
            origin: translation,
            ..Self::IDENTITY
        }
    }

    pub fn from_scale(scale: Vec2) -> Self {
        Self {
            x_axis: Vec2::new(scale.x, 0.0),
            y_axis: Vec2::new(0.0, scale.y),
            origin: Vec2::ZERO,
        }
    }

    pub fn from_rotation(angle_radians: f32) -> Self {
        let (sin, cos) = angle_radians.sin_cos();
        Self {
            x_axis: Vec2::new(cos, sin),
            y_axis: Vec2::new(-sin, cos),
            origin: Vec2::ZERO,
        }
    }

    /// Applies scale, then rotation, then translation (in that order).
    pub fn from_scale_rotation_origin(scale: Vec2, angle_radians: f32, origin: Vec2) -> Self {
        Self::from_translation(origin)
            * Self::from_rotation(angle_radians)
            * Self::from_scale(scale)
    }

    /// Applies rotation then translation.
    pub fn from_rotation_origin(angle_radians: f32, origin: Vec2) -> Self {
        Self::from_translation(origin) * Self::from_rotation(angle_radians)
    }

    pub fn is_identity(self) -> bool {
        self == Self::IDENTITY
    }

    pub fn transform_point(self, point: Vec2) -> Vec2 {
        self.x_axis * point.x + self.y_axis * point.y + self.origin
    }

    pub fn transform_vector(self, vector: Vec2) -> Vec2 {
        self.x_axis * vector.x + self.y_axis * vector.y
    }

    pub fn determinant(self) -> f32 {
        self.x_axis.x * self.y_axis.y - self.x_axis.y * self.y_axis.x
    }

    /// Returns the inverse, or `None` when the transform is degenerate
    /// (determinant ~0).
    pub fn try_inverse(self) -> Option<Self> {
        let det = self.determinant();
        if det.abs() < f32::EPSILON {
            return None;
        }
        let inv_det = 1.0 / det;
        let x_axis = Vec2::new(self.y_axis.y * inv_det, -self.x_axis.y * inv_det);
        let y_axis = Vec2::new(-self.y_axis.x * inv_det, self.x_axis.x * inv_det);
        let origin = -(x_axis * self.origin.x + y_axis * self.origin.y);
        Some(Self {
            x_axis,
            y_axis,
            origin,
        })
    }

    /// Inverse of `self`. Panics when the transform is degenerate; use
    /// [`Transform2D::try_inverse`] when that is possible.
    pub fn inverse(self) -> Self {
        self.try_inverse()
            .expect("Transform2D is not invertible (determinant ~0)")
    }

    /// Returns a copy translated by `offset` (applied after `self`).
    pub fn translated(self, offset: Vec2) -> Self {
        Self::from_translation(offset) * self
    }

    /// Returns a copy scaled about the local origin (applied before `self`).
    pub fn scaled(self, scale: Vec2) -> Self {
        self * Self::from_scale(scale)
    }

    /// Returns a copy rotated about the local origin (applied before `self`).
    pub fn rotated(self, angle_radians: f32) -> Self {
        self * Self::from_rotation(angle_radians)
    }

    pub fn to_array(self) -> [f32; 6] {
        [
            self.x_axis.x,
            self.x_axis.y,
            self.y_axis.x,
            self.y_axis.y,
            self.origin.x,
            self.origin.y,
        ]
    }
}

impl Mul for Transform2D {
    type Output = Self;

    /// `a * b` applies `b` first, then `a`.
    fn mul(self, rhs: Self) -> Self {
        Self {
            x_axis: self.transform_vector(rhs.x_axis),
            y_axis: self.transform_vector(rhs.y_axis),
            origin: self.transform_point(rhs.origin),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    fn approx(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < EPS
    }

    #[test]
    fn identity_transform() {
        let t = Transform2D::IDENTITY;
        assert!(t.is_identity());
        assert!(approx(
            t.transform_point(Vec2::new(3.0, 7.0)),
            Vec2::new(3.0, 7.0)
        ));
        assert!(approx(
            t.transform_vector(Vec2::new(3.0, 7.0)),
            Vec2::new(3.0, 7.0)
        ));
    }

    #[test]
    fn translate() {
        let t = Transform2D::from_translation(Vec2::new(10.0, -5.0));
        assert!(approx(
            t.transform_point(Vec2::new(1.0, 1.0)),
            Vec2::new(11.0, -4.0)
        ));
        // vectors are unaffected by translation
        assert!(approx(
            t.transform_vector(Vec2::new(1.0, 1.0)),
            Vec2::new(1.0, 1.0)
        ));
    }

    #[test]
    fn scale() {
        let t = Transform2D::from_scale(Vec2::new(2.0, 3.0));
        assert!(approx(
            t.transform_point(Vec2::new(1.0, 1.0)),
            Vec2::new(2.0, 3.0)
        ));
    }

    #[test]
    fn rotation() {
        let t = Transform2D::from_rotation(std::f32::consts::FRAC_PI_2);
        assert!(approx(
            t.transform_point(Vec2::new(1.0, 0.0)),
            Vec2::new(0.0, 1.0)
        ));
        assert!(approx(
            t.transform_point(Vec2::new(0.0, 1.0)),
            Vec2::new(-1.0, 0.0)
        ));
    }

    #[test]
    fn composition_order() {
        // T * R: rotate first, then translate.
        let t = Transform2D::from_translation(Vec2::new(10.0, 0.0))
            * Transform2D::from_rotation(std::f32::consts::FRAC_PI_2);
        assert!(approx(
            t.transform_point(Vec2::new(1.0, 0.0)),
            Vec2::new(10.0, 1.0)
        ));

        // R * T: translate first, then rotate.
        let t = Transform2D::from_rotation(std::f32::consts::FRAC_PI_2)
            * Transform2D::from_translation(Vec2::new(10.0, 0.0));
        assert!(approx(
            t.transform_point(Vec2::new(1.0, 0.0)),
            Vec2::new(0.0, 11.0)
        ));
    }

    #[test]
    fn scale_rotation_origin() {
        let t = Transform2D::from_scale_rotation_origin(
            Vec2::new(2.0, 2.0),
            std::f32::consts::FRAC_PI_2,
            Vec2::new(5.0, 5.0),
        );
        // scale: (1,0) -> (2,0); rotate 90deg -> (0,2); translate -> (5,7)
        assert!(approx(
            t.transform_point(Vec2::new(1.0, 0.0)),
            Vec2::new(5.0, 7.0)
        ));
    }

    #[test]
    fn inverse() {
        let t = Transform2D::from_scale_rotation_origin(
            Vec2::new(2.0, 3.0),
            0.7,
            Vec2::new(10.0, -4.0),
        );
        let inv = t.inverse();
        let p = Vec2::new(3.0, 8.0);
        assert!(approx(inv.transform_point(t.transform_point(p)), p));
        assert!(approx(t.transform_point(inv.transform_point(p)), p));
    }

    #[test]
    fn inverse_degenerate_is_none() {
        let t = Transform2D::from_scale(Vec2::new(0.0, 1.0));
        assert_eq!(t.try_inverse(), None);
    }

    #[test]
    fn helpers_match_composition() {
        let t = Transform2D::from_translation(Vec2::new(1.0, 2.0));
        assert_eq!(
            t.translated(Vec2::new(3.0, 4.0)),
            Transform2D::from_translation(Vec2::new(4.0, 6.0))
        );

        let base = Transform2D::from_translation(Vec2::new(1.0, 2.0));
        assert_eq!(
            base.scaled(Vec2::new(2.0, 2.0)),
            base * Transform2D::from_scale(Vec2::new(2.0, 2.0))
        );
        assert_eq!(base.rotated(1.0), base * Transform2D::from_rotation(1.0));
    }
}
