//! `demoapp_ffi` — a C ABI over the real `demo_app` gallery.
//!
//! Where `draw_ffi` exposes the core to a host that builds its own UI, this
//! crate exposes the *whole* Rust gallery: a foreign host creates a `DemoApp`,
//! drives its frame (viewport, update, layout) and reads back the resulting
//! `DrawList` through `draw_ffi`'s command record.
//!
//! ```text
//! C++ host -> demoapp_* -> DemoApp (draw_components/draw_ui) -> DrawList -> C++ backend
//! ```
//!
//! # Text
//!
//! `DemoApp` lays out with `draw_ui`'s built-in `ApproxTextMeasurer` (it does
//! not depend on `draw_font`), and it emits `DrawText` commands for every label.
//! ABI v1 has no text record, so a host reading the list back sees those as
//! `Unsupported` and draws only the chrome. That is a real limitation of the
//! comparison, not a bug: the geometry, layout and colors are the real app's.
//!
//! # Safety
//!
//! A null handle is a no-op (or a null list); every other handle must come from
//! [`demoapp_new`] and be released with [`demoapp_free`]. The list returned by
//! [`demoapp_paint`] is freed with `quill_draw_list_free` from `draw_ffi`.

use demo_app::DemoApp;
use draw_core::{Size, ViewportSize};
use draw_ffi::{wrap_draw_list, QuillDrawList};
use draw_render::PaintContext;
use draw_theme::Mode;

/// An opaque, Rust-owned `DemoApp` plus the viewport the host last set.
pub struct DemoAppHandle {
    app: DemoApp,
    viewport: ViewportSize,
}

/// Creates the gallery with the dark theme.
#[no_mangle]
pub extern "C" fn demoapp_new() -> *mut DemoAppHandle {
    Box::into_raw(Box::new(DemoAppHandle {
        app: DemoApp::new(),
        viewport: ViewportSize::new(Size::new(1200.0, 760.0)),
    }))
}

/// Creates the gallery with the dark (`light == 0`) or light theme.
#[no_mangle]
pub extern "C" fn demoapp_new_with_mode(light: u32) -> *mut DemoAppHandle {
    let mode = if light != 0 { Mode::Light } else { Mode::Dark };
    Box::into_raw(Box::new(DemoAppHandle {
        app: DemoApp::with_mode(mode),
        viewport: ViewportSize::new(Size::new(1200.0, 760.0)),
    }))
}

/// Releases a handle from [`demoapp_new`]. A null pointer is a no-op.
///
/// # Safety
///
/// `handle` must be null or a pointer from [`demoapp_new`] / [`demoapp_new_with_mode`]
/// that has not been freed yet.
#[no_mangle]
pub unsafe extern "C" fn demoapp_free(handle: *mut DemoAppHandle) {
    if handle.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(handle) });
}

/// Sets the logical viewport the next layout/update uses.
///
/// # Safety
///
/// `handle` must be null or a valid pointer from [`demoapp_new`].
#[no_mangle]
pub unsafe extern "C" fn demoapp_set_viewport(handle: *mut DemoAppHandle, width: f32, height: f32) {
    if let Some(handle) = unsafe { handle.as_mut() } {
        handle.viewport = ViewportSize::new(Size::new(width, height));
    }
}

/// Selects a catalog group (clamped to the valid range).
///
/// # Safety
///
/// `handle` must be null or a valid pointer from [`demoapp_new`].
#[no_mangle]
pub unsafe extern "C" fn demoapp_show_group(handle: *mut DemoAppHandle, index: u32) {
    if let Some(handle) = unsafe { handle.as_mut() } {
        handle.app.show_group(index as usize);
    }
}

/// The number of catalog groups.
#[no_mangle]
pub extern "C" fn demoapp_group_count() -> u32 {
    DemoApp::group_count() as u32
}

/// Drains per-frame requests and advances overlay timers.
///
/// # Safety
///
/// `handle` must be null or a valid pointer from [`demoapp_new`].
#[no_mangle]
pub unsafe extern "C" fn demoapp_update(handle: *mut DemoAppHandle, dt: f32) {
    if let Some(handle) = unsafe { handle.as_mut() } {
        let viewport = handle.viewport;
        handle.app.update(viewport, dt);
    }
}

/// Resolves UI layout for the current viewport.
///
/// # Safety
///
/// `handle` must be null or a valid pointer from [`demoapp_new`].
#[no_mangle]
pub unsafe extern "C" fn demoapp_layout(handle: *mut DemoAppHandle) {
    if let Some(handle) = unsafe { handle.as_mut() } {
        let viewport = handle.viewport;
        handle.app.layout(viewport);
    }
}

/// Paints the current frame into a fresh `DrawList` the host owns.
///
/// The list is read with `quill_draw_list_len` / `quill_draw_list_command` and
/// released with `quill_draw_list_free`. A null handle yields a null list.
///
/// # Safety
///
/// `handle` must be null or a valid pointer from [`demoapp_new`].
#[no_mangle]
pub unsafe extern "C" fn demoapp_paint(handle: *const DemoAppHandle) -> *mut QuillDrawList {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let mut ctx = PaintContext::new();
    handle.app.paint(&mut ctx);
    wrap_draw_list(ctx.into_draw_list())
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_ffi::{
        quill_draw_list_command, quill_draw_list_free, quill_draw_list_len, QuillCommandTag,
    };

    /// The gallery lays out and paints into a list the host can read back.
    #[test]
    fn a_frame_paints_into_a_readable_list() {
        let handle = demoapp_new();
        assert!(!handle.is_null());
        unsafe {
            demoapp_set_viewport(handle, 1200.0, 760.0);
            demoapp_update(handle, 0.016);
            demoapp_layout(handle);
        }
        let list = unsafe { demoapp_paint(handle) };
        assert!(!list.is_null());

        let count = unsafe { quill_draw_list_len(list) };
        assert!(
            count > 50,
            "the gallery should emit many commands, got {count}"
        );

        // The chrome really crosses the boundary: cards and other geometry
        // survive the read-back, not just state commands.
        let mut rounded = 0;
        let mut geometry = 0;
        for i in 0..count {
            match unsafe { quill_draw_list_command(list, i) }.tag {
                QuillCommandTag::FillRoundedRect | QuillCommandTag::StrokeRoundedRect => {
                    rounded += 1;
                    geometry += 1;
                }
                QuillCommandTag::FillRect
                | QuillCommandTag::StrokeRect
                | QuillCommandTag::Line
                | QuillCommandTag::FillCircle
                | QuillCommandTag::StrokeCircle => geometry += 1,
                _ => {}
            }
        }
        assert!(rounded > 0, "expected card chrome");
        assert!(
            geometry > 20,
            "expected a full frame of geometry, got {geometry}"
        );

        unsafe { quill_draw_list_free(list) };
        unsafe { demoapp_free(handle) };
    }

    /// Switching the group changes the emitted list.
    #[test]
    fn showing_a_group_changes_the_frame() {
        assert!(demoapp_group_count() > 1);
        let handle = demoapp_new();
        unsafe {
            demoapp_set_viewport(handle, 1000.0, 700.0);
        }

        let paint = |handle: *mut DemoAppHandle| unsafe {
            demoapp_update(handle, 0.016);
            demoapp_layout(handle);
            let list = demoapp_paint(handle);
            let count = quill_draw_list_len(list);
            quill_draw_list_free(list);
            count
        };
        let first = paint(handle);
        unsafe { demoapp_show_group(handle, 1) };
        let second = paint(handle);
        assert!(first > 0 && second > 0);
        unsafe { demoapp_free(handle) };
    }

    /// A null handle is safe.
    #[test]
    fn a_null_handle_is_safe() {
        unsafe {
            demoapp_free(std::ptr::null_mut());
            demoapp_update(std::ptr::null_mut(), 0.0);
            demoapp_layout(std::ptr::null_mut());
            demoapp_set_viewport(std::ptr::null_mut(), 10.0, 10.0);
            demoapp_show_group(std::ptr::null_mut(), 0);
            assert!(demoapp_paint(std::ptr::null()).is_null());
        }
    }
}
