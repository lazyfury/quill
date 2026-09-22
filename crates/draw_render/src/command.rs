use draw_core::{Color, FontWeight, Rect, Transform2D, Vec2};

use crate::texture::TextureId;

/// A fill/stroke style.
///
/// MVP only carries a solid color; gradients/patterns can grow here later
/// without changing the command shapes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    pub color: Color,
}

impl Paint {
    pub const TRANSPARENT: Self = Self::new(Color::TRANSPARENT);

    pub const fn new(color: Color) -> Self {
        Self { color }
    }

    pub const fn solid(color: Color) -> Self {
        Self::new(color)
    }

    pub const fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(Color::rgb(r, g, b))
    }
}

impl Default for Paint {
    fn default() -> Self {
        Self::new(Color::WHITE)
    }
}

impl From<Color> for Paint {
    fn from(color: Color) -> Self {
        Self::new(color)
    }
}

/// Horizontal text alignment relative to the draw position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Per-corner radii for a rounded rectangle, in logical pixels.
///
/// Order is clockwise from the top-left. A value of `0.0` is a square corner,
/// which lets one shape mix square and rounded corners (for example a list item
/// with square left corners and rounded right corners).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadii {
    /// All corners square.
    pub const ZERO: Self = Self::uniform(0.0);

    pub const fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    /// The same radius on every corner.
    pub const fn uniform(radius: f32) -> Self {
        Self::new(radius, radius, radius, radius)
    }

    pub fn is_zero(self) -> bool {
        self.top_left == 0.0
            && self.top_right == 0.0
            && self.bottom_right == 0.0
            && self.bottom_left == 0.0
    }

    /// Clamps every corner to `max` (typically half the smaller side).
    pub fn clamp(self, max: f32) -> Self {
        Self::new(
            self.top_left.clamp(0.0, max),
            self.top_right.clamp(0.0, max),
            self.bottom_right.clamp(0.0, max),
            self.bottom_left.clamp(0.0, max),
        )
    }

    /// Shrinks every corner by `amount` (never below zero).
    pub fn inset(self, amount: f32) -> Self {
        Self::new(
            (self.top_left - amount).max(0.0),
            (self.top_right - amount).max(0.0),
            (self.bottom_right - amount).max(0.0),
            (self.bottom_left - amount).max(0.0),
        )
    }
}

impl From<f32> for CornerRadii {
    fn from(radius: f32) -> Self {
        Self::uniform(radius)
    }
}

/// A single backend-neutral draw operation.
///
/// Commands are interpreted in order, and `Save`/`Restore` form a balanced
/// stack that backends use to push/pop transform, opacity and clip state.
///
/// # Coordinate spaces
///
/// - Geometry (`rect`, `center`, `destination`) is in the **current transform
///   space**, i.e. it is affected by the most recent `SetTransform`.
/// - `ClipRect` is in **viewport/logical space** (already transformed); backends
///   apply it directly.
/// - This type contains no Canvas/WebGL/WGPU/DOM objects.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// Pushes the current transform/opacity/clip onto the backend stack.
    Save,
    /// Pops the transform/opacity/clip pushed by the matching `Save`.
    Restore,
    /// Replaces the current transform.
    SetTransform(Transform2D),
    /// Replaces the current opacity (multiplier, `0.0..=1.0`).
    SetOpacity(f32),
    /// Sets the clip rectangle in viewport/logical space.
    ClipRect(Rect),
    FillRect {
        rect: Rect,
        paint: Paint,
    },
    StrokeRect {
        rect: Rect,
        paint: Paint,
        width: f32,
    },
    /// A stroked line segment from `from` to `to`, `width` logical pixels wide.
    Line {
        from: Vec2,
        to: Vec2,
        paint: Paint,
        width: f32,
    },
    FillCircle {
        center: Vec2,
        radius: f32,
        paint: Paint,
    },
    StrokeCircle {
        center: Vec2,
        radius: f32,
        paint: Paint,
        width: f32,
    },
    /// A filled rounded rectangle. Corner radii are clamped to half the smaller
    /// side.
    FillRoundedRect {
        rect: Rect,
        corners: CornerRadii,
        paint: Paint,
    },
    /// A stroked rounded rectangle. `width` is the stroke thickness; corner
    /// radii refer to the outer corners.
    StrokeRoundedRect {
        rect: Rect,
        corners: CornerRadii,
        paint: Paint,
        width: f32,
    },
    DrawImage {
        texture: TextureId,
        /// Destination rectangle in the current transform space.
        destination: Rect,
        /// Optional source sub-rectangle within the texture.
        source: Option<Rect>,
        paint: Paint,
    },
    DrawText {
        text: String,
        /// Baseline origin in the current transform space.
        position: Vec2,
        font_size: f32,
        /// Regular or bold.
        weight: FontWeight,
        align: TextAlign,
        paint: Paint,
    },
}
