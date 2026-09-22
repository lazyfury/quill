//! The `extern "C"` surface: allocation, the state/geometry builders, and the
//! read-back of the command stream.
//!
//! The functions take raw pointers. A null `DrawList` is a no-op (or a zeroed
//! [`QuillCommand`] with [`QuillCommandTag::Unsupported`]), so a host cannot
//! segfault by forgetting a null check; every other pointer must come from
//! [`quill_draw_list_new`] and be released with [`quill_draw_list_free`].

use draw_render::DrawCommand;

use crate::convert::*;
use crate::types::*;

/// Returns [`ABI_VERSION`] so a host can check its header matches.
#[no_mangle]
pub extern "C" fn quill_abi_version() -> u32 {
    ABI_VERSION
}

/// Allocates an empty [`DrawList`](draw_render::DrawList). Release it with
/// [`quill_draw_list_free`].
#[no_mangle]
pub extern "C" fn quill_draw_list_new() -> *mut QuillDrawList {
    Box::into_raw(Box::new(QuillDrawList::new()))
}

/// Releases a list from [`quill_draw_list_new`]. A null pointer is a no-op.
///
/// # Safety
///
/// `list` must be null or a pointer from [`quill_draw_list_new`] that has not
/// been freed yet.
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_free(list: *mut QuillDrawList) {
    if list.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(list) });
}

/// Removes every command, keeping the allocation.
///
/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_clear(list: *mut QuillDrawList) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.clear();
    }
}

/// The number of commands; `0` for a null list.
///
/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_len(list: *const QuillDrawList) -> usize {
    unsafe { list.as_ref() }.map_or(0, |list| list.list.len())
}

/// Reads one command back as a flat record.
///
/// An out-of-bounds `index` (or a null list) yields
/// [`QuillCommandTag::Unsupported`] with every field zeroed.
///
/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_command(
    list: *const QuillDrawList,
    index: usize,
) -> QuillCommand {
    let Some(list) = (unsafe { list.as_ref() }) else {
        return QuillCommand {
            tag: QuillCommandTag::Unsupported,
            ..QuillCommand::default()
        };
    };
    match list.list.commands().get(index) {
        Some(command) => command_record(command),
        None => QuillCommand {
            tag: QuillCommandTag::Unsupported,
            ..QuillCommand::default()
        },
    }
}

// -- state commands --------------------------------------------------------

/// Pushes the transform/opacity/clip state.
///
/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_save(list: *mut QuillDrawList) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::Save);
    }
}

/// Pops the state pushed by a matching save.
///
/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_restore(list: *mut QuillDrawList) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::Restore);
    }
}

/// Replaces the current transform.
///
/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_set_transform(
    list: *mut QuillDrawList,
    transform: QuillTransform,
) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list
            .push(DrawCommand::SetTransform(to_transform(transform)));
    }
}

/// Replaces the current opacity multiplier (`0.0..=1.0`).
///
/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_set_opacity(list: *mut QuillDrawList, opacity: f32) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::SetOpacity(opacity));
    }
}

/// Sets the clip rectangle (viewport/logical space).
///
/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_clip_rect(list: *mut QuillDrawList, rect: QuillRect) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::ClipRect(to_rect(rect)));
    }
}

// -- geometry commands -----------------------------------------------------

/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_fill_rect(
    list: *mut QuillDrawList,
    rect: QuillRect,
    paint: QuillPaint,
) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::FillRect {
            rect: to_rect(rect),
            paint: to_paint(paint),
        });
    }
}

/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_stroke_rect(
    list: *mut QuillDrawList,
    rect: QuillRect,
    paint: QuillPaint,
    width: f32,
) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::StrokeRect {
            rect: to_rect(rect),
            paint: to_paint(paint),
            width,
        });
    }
}

/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_line(
    list: *mut QuillDrawList,
    from: QuillVec2,
    to: QuillVec2,
    paint: QuillPaint,
    width: f32,
) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::Line {
            from: to_vec2(from),
            to: to_vec2(to),
            paint: to_paint(paint),
            width,
        });
    }
}

/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_fill_circle(
    list: *mut QuillDrawList,
    center: QuillVec2,
    radius: f32,
    paint: QuillPaint,
) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::FillCircle {
            center: to_vec2(center),
            radius,
            paint: to_paint(paint),
        });
    }
}

/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_stroke_circle(
    list: *mut QuillDrawList,
    center: QuillVec2,
    radius: f32,
    paint: QuillPaint,
    width: f32,
) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::StrokeCircle {
            center: to_vec2(center),
            radius,
            paint: to_paint(paint),
            width,
        });
    }
}

/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_fill_rounded_rect(
    list: *mut QuillDrawList,
    rect: QuillRect,
    corners: QuillCornerRadii,
    paint: QuillPaint,
) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::FillRoundedRect {
            rect: to_rect(rect),
            corners: to_corners(corners),
            paint: to_paint(paint),
        });
    }
}

/// # Safety
///
/// `list` must be null or a valid pointer from [`quill_draw_list_new`].
#[no_mangle]
pub unsafe extern "C" fn quill_draw_list_stroke_rounded_rect(
    list: *mut QuillDrawList,
    rect: QuillRect,
    corners: QuillCornerRadii,
    paint: QuillPaint,
    width: f32,
) {
    if let Some(list) = unsafe { list.as_mut() } {
        list.list.push(DrawCommand::StrokeRoundedRect {
            rect: to_rect(rect),
            corners: to_corners(corners),
            paint: to_paint(paint),
            width,
        });
    }
}
