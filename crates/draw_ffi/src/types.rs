//! The `#[repr(C)]` ABI types and the opaque draw list.
//!
//! Everything here is made of `f32`, a C enum or an opaque pointer, so the
//! layout is stable and mirrored by `include/quill.h`. If you change a field,
//! change the header and bump [`ABI_VERSION`].

use draw_render::DrawList;

/// Version of this ABI. Bump on any layout or signature change; a host should
/// refuse to run when `quill_abi_version()` disagrees with its header.
pub const ABI_VERSION: u32 = 1;

/// The C-visible command discriminator.
///
/// Discriminants are part of the ABI: `include/quill.h` mirrors them and the
/// C++ backend switches on them.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuillCommandTag {
    #[default]
    Save = 0,
    Restore = 1,
    SetTransform = 2,
    SetOpacity = 3,
    ClipRect = 4,
    FillRect = 5,
    StrokeRect = 6,
    Line = 7,
    FillCircle = 8,
    StrokeCircle = 9,
    FillRoundedRect = 10,
    StrokeRoundedRect = 11,
    /// A command ABI v1 does not model (`DrawImage` / `DrawText`). The record is
    /// otherwise zeroed; a backend skips it.
    Unsupported = 12,
}

/// A 2D point/vector in logical pixels.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QuillVec2 {
    pub x: f32,
    pub y: f32,
}

/// An axis-aligned rectangle in logical pixels.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QuillRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// An RGBA color, components in `0.0..=1.0`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QuillColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// A 2D affine transform: two basis axes plus an origin (six floats).
///
/// A point maps to `x_axis * p.x + y_axis * p.y + origin`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QuillTransform {
    pub x_axis: QuillVec2,
    pub y_axis: QuillVec2,
    pub origin: QuillVec2,
}

/// Per-corner radii, clockwise from the top-left, in logical pixels.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QuillCornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

/// A solid fill/stroke style.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QuillPaint {
    pub color: QuillColor,
}

/// One command, flattened to a fixed layout.
///
/// Only the fields named by [`tag`](Self::tag) are meaningful; every other
/// field is zeroed. See `include/quill.h` for the field-to-command map.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QuillCommand {
    pub tag: QuillCommandTag,
    pub transform: QuillTransform,
    pub rect: QuillRect,
    pub corners: QuillCornerRadii,
    pub from: QuillVec2,
    pub to: QuillVec2,
    pub center: QuillVec2,
    pub paint: QuillPaint,
    pub radius: f32,
    pub width: f32,
    pub opacity: f32,
}

/// An opaque, Rust-owned [`DrawList`].
pub struct QuillDrawList {
    pub(crate) list: DrawList,
}

impl QuillDrawList {
    pub(crate) fn new() -> Self {
        Self {
            list: DrawList::new(),
        }
    }
}

impl Default for QuillDrawList {
    fn default() -> Self {
        Self::new()
    }
}

/// Wraps a [`DrawList`] built by another Rust crate for a foreign host.
///
/// This is a Rust helper, not part of the C ABI. A companion FFI crate (e.g.
/// `demoapp_ffi`) builds a list from a Rust app and hands it to the same
/// [`quill_draw_list_command`](crate::quill_draw_list_command) read-back; the
/// host releases it with [`quill_draw_list_free`](crate::quill_draw_list_free).
pub fn wrap_draw_list(list: DrawList) -> *mut QuillDrawList {
    Box::into_raw(Box::new(QuillDrawList { list }))
}
