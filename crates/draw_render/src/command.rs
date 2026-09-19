use draw_core::{Color, Rect, Transform2D, Vec2};

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
        align: TextAlign,
        paint: Paint,
    },
}
