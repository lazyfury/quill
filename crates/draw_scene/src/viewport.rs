use draw_core::{Rect, Size, Transform2D, Vec2};

/// The root render context of a [`crate::SceneTree`].
///
/// A `Viewport` owns the logical drawing area (`size`) and the
/// `canvas_transform` that maps **world** coordinates to **screen** (logical
/// viewport) coordinates. The default world canvas (Godot layer `0`) is painted
/// through this transform; a `CanvasLayer` subtree uses its own transform
/// instead (Phase 3).
///
/// `canvas_transform` is written by [`crate::SceneTree::update`] from the
/// current `Camera2D` (Godot `Camera2D::get_camera_transform`, which returns
/// the affine inverse of the camera's world transform). With no current camera
/// it stays the identity.
///
/// The root instance is the tree's root node, a [`crate::NodeKind::Viewport`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    size: Size,
    canvas_transform: Transform2D,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            size: Size::ZERO,
            canvas_transform: Transform2D::IDENTITY,
        }
    }
}

impl Viewport {
    pub const fn new(size: Size) -> Self {
        Self {
            size,
            canvas_transform: Transform2D::IDENTITY,
        }
    }

    /// Logical drawing area in pixels.
    pub const fn size(self) -> Size {
        self.size
    }

    pub fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    /// World -> screen transform, valid after [`crate::SceneTree::update`].
    pub const fn canvas_transform(self) -> Transform2D {
        self.canvas_transform
    }

    pub(crate) fn set_canvas_transform(&mut self, transform: Transform2D) {
        self.canvas_transform = transform;
    }

    /// The whole viewport as a rectangle starting at the origin, in screen
    /// (logical viewport) coordinates.
    pub fn rect(self) -> Rect {
        Rect::from_min_size(Vec2::ZERO, self.size)
    }

    /// Maps a world-space point to screen (logical viewport) coordinates.
    pub fn world_to_screen(self, point: Vec2) -> Vec2 {
        self.canvas_transform.transform_point(point)
    }

    /// Maps a screen (logical viewport) point to world coordinates.
    pub fn screen_to_world(self, point: Vec2) -> Vec2 {
        self.canvas_transform.inverse().transform_point(point)
    }
}
