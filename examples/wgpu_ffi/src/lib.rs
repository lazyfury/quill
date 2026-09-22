//! `wgpu_ffi` — a C ABI over [`draw_backend_wgpu`].
//!
//! The C++ host owns the window; this crate owns the wgpu instance, surface,
//! device and the Rust renderer. A frame is:
//!
//! ```text
//! C++ builds a DrawList -> wgpu_ffi_render(handle, list) -> draw_backend_wgpu -> surface
//! ```
//!
//! Two entry points, one backend:
//!
//! - **Surface** (`wgpu_ffi_new` with a native view): `wgpu_ffi_render` gets the
//!   next surface texture, renders the list and presents it.
//! - **Offscreen** (`wgpu_ffi_new` with a null view): `wgpu_ffi_render_offscreen`
//!   renders into the backend's own texture and `wgpu_ffi_read_pixels` reads it
//!   back — the no-screenshot verification, and a direct comparison with the
//!   C++ OpenGL backend's pixels.
//!
//! The list is the same opaque `QuillDrawList` the host built with `draw_ffi`;
//! this crate reads it through [`draw_ffi::QuillDrawList::draw_list`].
//!
//! # Platform
//!
//! The surface path takes an AppKit `NSView` pointer (macOS), matching the
//! C++ host. The offscreen path is platform-independent.
//!
//! # Safety
//!
//! Handles come from [`wgpu_ffi_new`] and are released with [`wgpu_ffi_free`];
//! the native view must outlive the handle. A null handle or list is a no-op.

use std::ffi::c_void;
use std::ptr::NonNull;

use draw_backend_wgpu::{wgpu, WgpuBackend};
use draw_core::{Color, Size, ViewportSize};
use draw_ffi::QuillDrawList;
use draw_render::RenderBackend;
use wgpu::rwh::{AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle};

/// An opaque, Rust-owned wgpu renderer: instance, optional surface, backend.
pub struct WgpuFfi {
    /// Kept so the instance outlives the surface. The backend clones it too.
    _instance: wgpu::Instance,
    backend: WgpuBackend,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    scale: f32,
}

impl WgpuFfi {
    fn create(ns_view: *mut c_void, width: u32, height: u32, scale: f32) -> Result<Self, String> {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let instance = wgpu::Instance::default();

        // A null view means headless: offscreen rendering only.
        if ns_view.is_null() {
            let mut backend =
                WgpuBackend::from_instance(&instance, None, wgpu::PowerPreference::HighPerformance)
                    .map_err(|error| error.to_string())?;
            backend.set_scale_factor(scale);
            return Ok(Self {
                _instance: instance,
                backend,
                surface: None,
                config: None,
                scale,
            });
        }

        let raw_handle = AppKitWindowHandle::new(
            NonNull::new(ns_view).ok_or_else(|| "null NSView".to_string())?,
        );
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
                raw_window_handle: RawWindowHandle::AppKit(raw_handle),
            })
        }
        .map_err(|error| error.to_string())?;

        let mut backend = WgpuBackend::from_instance(
            &instance,
            Some(&surface),
            wgpu::PowerPreference::HighPerformance,
        )
        .map_err(|error| error.to_string())?;

        let capabilities = surface.get_capabilities(backend.adapter());
        // Prefer a non-sRGB surface so the colors match the C++ OpenGL backend.
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .unwrap_or(capabilities.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: Vec::new(),
        };
        surface.configure(backend.device(), &config);
        backend.set_scale_factor(scale);

        Ok(Self {
            _instance: instance,
            backend,
            surface: Some(surface),
            config: Some(config),
            scale,
        })
    }
}

/// Creates the renderer. Pass a native `NSView` pointer for a window surface,
/// or null for a headless (offscreen-only) backend. Returns null on failure.
#[no_mangle]
pub extern "C" fn wgpu_ffi_new(
    ns_view: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut WgpuFfi {
    match WgpuFfi::create(ns_view, width, height, scale) {
        Ok(handle) => Box::into_raw(Box::new(handle)),
        Err(error) => {
            eprintln!("wgpu_ffi: init failed: {error}");
            std::ptr::null_mut()
        }
    }
}

/// Releases a handle from [`wgpu_ffi_new`]. A null pointer is a no-op.
///
/// # Safety
///
/// `handle` must be null or a pointer from [`wgpu_ffi_new`] that has not been
/// freed, and the native view must still be alive.
#[no_mangle]
pub unsafe extern "C" fn wgpu_ffi_free(handle: *mut WgpuFfi) {
    if handle.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(handle) });
}

/// Reconfigures the surface for a new device-pixel size / scale factor.
///
/// # Safety
///
/// `handle` must be null or a valid pointer from [`wgpu_ffi_new`].
#[no_mangle]
pub unsafe extern "C" fn wgpu_ffi_resize(
    handle: *mut WgpuFfi,
    width: u32,
    height: u32,
    scale: f32,
) {
    let Some(handle) = (unsafe { handle.as_mut() }) else {
        return;
    };
    handle.scale = if scale > 0.0 { scale } else { 1.0 };
    handle.backend.set_scale_factor(handle.scale);
    if let (Some(surface), Some(config)) = (&handle.surface, &mut handle.config) {
        config.width = width.max(1);
        config.height = height.max(1);
        surface.configure(handle.backend.device(), config);
    }
}

/// Renders one list to the window surface and presents it.
///
/// Returns `0` on success and a negative code on failure (no surface, a lost
/// surface, or a backend error). `clear` is the background color.
///
/// # Safety
///
/// `handle` and `list` must be null or valid pointers; `list` stays owned by
/// the caller.
#[no_mangle]
pub unsafe extern "C" fn wgpu_ffi_render(
    handle: *mut WgpuFfi,
    list: *const QuillDrawList,
    clear_r: f32,
    clear_g: f32,
    clear_b: f32,
    clear_a: f32,
) -> i32 {
    let Some(handle) = (unsafe { handle.as_mut() }) else {
        return -1;
    };
    let Some(list) = (unsafe { list.as_ref() }) else {
        return -1;
    };
    let (Some(surface), Some(config)) = (&handle.surface, &handle.config) else {
        return -2;
    };
    handle
        .backend
        .set_clear_color(Color::new(clear_r, clear_g, clear_b, clear_a));

    let frame = match surface.get_current_texture() {
        Ok(frame) => frame,
        Err(error) => {
            eprintln!("wgpu_ffi: get_current_texture: {error}");
            return -3;
        }
    };
    let view = frame.texture.create_view(&Default::default());
    let viewport = ViewportSize::new(Size::new(
        config.width as f32 / handle.scale,
        config.height as f32 / handle.scale,
    ));
    if let Err(error) = handle.backend.begin_frame_with_view(
        view,
        config.width,
        config.height,
        config.format,
        viewport,
    ) {
        eprintln!("wgpu_ffi: begin_frame: {error}");
        return -4;
    }
    if let Err(error) = handle.backend.submit(list.draw_list()) {
        eprintln!("wgpu_ffi: submit: {error}");
        return -5;
    }
    if let Err(error) = handle.backend.end_frame() {
        eprintln!("wgpu_ffi: end_frame: {error}");
        return -6;
    }
    frame.present();
    0
}

/// Renders one list into the backend's offscreen texture.
///
/// `width`/`height` are device pixels; `scale` maps them to the logical
/// viewport. Read the result with [`wgpu_ffi_read_pixels`].
///
/// # Safety
///
/// `handle` and `list` must be null or valid pointers.
#[no_mangle]
pub unsafe extern "C" fn wgpu_ffi_render_offscreen(
    handle: *mut WgpuFfi,
    list: *const QuillDrawList,
    width: u32,
    height: u32,
    scale: f32,
    clear_r: f32,
    clear_g: f32,
    clear_b: f32,
    clear_a: f32,
) -> i32 {
    let Some(handle) = (unsafe { handle.as_mut() }) else {
        return -1;
    };
    let Some(list) = (unsafe { list.as_ref() }) else {
        return -1;
    };
    let scale = if scale > 0.0 { scale } else { 1.0 };
    handle.scale = scale;
    handle.backend.set_scale_factor(scale);
    handle
        .backend
        .set_clear_color(Color::new(clear_r, clear_g, clear_b, clear_a));

    let viewport = ViewportSize::new(Size::new(
        width.max(1) as f32 / scale,
        height.max(1) as f32 / scale,
    ));
    if let Err(error) = handle.backend.begin_frame(viewport) {
        eprintln!("wgpu_ffi: begin_frame: {error}");
        return -2;
    }
    if let Err(error) = handle.backend.submit(list.draw_list()) {
        eprintln!("wgpu_ffi: submit: {error}");
        return -3;
    }
    if let Err(error) = handle.backend.end_frame() {
        eprintln!("wgpu_ffi: end_frame: {error}");
        return -4;
    }
    0
}

/// Copies the last offscreen frame into `out` as tightly packed RGBA8.
///
/// Returns the number of bytes written (0 on failure or a non-offscreen
/// frame). `out` must be at least `width * height * 4` bytes.
///
/// # Safety
///
/// `handle` must be null or a valid pointer; `out` must be valid for
/// `out_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn wgpu_ffi_read_pixels(
    handle: *mut WgpuFfi,
    out: *mut u8,
    out_len: usize,
) -> usize {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    match handle.backend.read_pixels() {
        Ok(buffer) => {
            let count = buffer.data.len().min(out_len);
            unsafe { std::ptr::copy_nonoverlapping(buffer.data.as_ptr(), out, count) };
            count
        }
        Err(error) => {
            eprintln!("wgpu_ffi: read_pixels: {error}");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_ffi::{
        quill_draw_list_fill_rect, quill_draw_list_free, quill_draw_list_new, QuillColor,
        QuillPaint, QuillRect,
    };

    fn red() -> QuillPaint {
        QuillPaint {
            color: QuillColor {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        }
    }

    /// A headless handle renders a list offscreen and reads it back: the same
    /// `DrawList` -> pixels path the C++ host uses, with no window.
    #[test]
    fn offscreen_render_reads_back_the_drawn_color() {
        let handle = wgpu_ffi_new(std::ptr::null_mut(), 0, 0, 1.0);
        if handle.is_null() {
            // No GPU on the test machine; nothing to verify.
            return;
        }
        let list = quill_draw_list_new();
        unsafe {
            // A red 8x8 square at (4, 4) on a black clear.
            quill_draw_list_fill_rect(
                list,
                QuillRect {
                    x: 4.0,
                    y: 4.0,
                    width: 8.0,
                    height: 8.0,
                },
                red(),
            );
        }
        let rc =
            unsafe { wgpu_ffi_render_offscreen(handle, list, 32, 32, 1.0, 0.0, 0.0, 0.0, 1.0) };
        assert_eq!(rc, 0, "offscreen render should succeed");

        let mut pixels = vec![0u8; 32 * 32 * 4];
        let written = unsafe { wgpu_ffi_read_pixels(handle, pixels.as_mut_ptr(), pixels.len()) };
        assert_eq!(written, pixels.len());

        let at = |x: usize, y: usize| {
            let i = (y * 32 + x) * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        let inside = at(8, 8);
        assert!(
            inside[0] > 200 && inside[1] < 40 && inside[2] < 40,
            "center should be red, got {inside:?}"
        );
        let outside = at(28, 28);
        assert!(
            outside[0] < 20 && outside[1] < 20 && outside[2] < 20,
            "corner should be the black clear, got {outside:?}"
        );

        unsafe { quill_draw_list_free(list) };
        unsafe { wgpu_ffi_free(handle) };
    }

    /// A null handle or list is safe.
    #[test]
    fn a_null_handle_is_safe() {
        unsafe {
            wgpu_ffi_free(std::ptr::null_mut());
            wgpu_ffi_resize(std::ptr::null_mut(), 1, 1, 1.0);
            assert_eq!(
                wgpu_ffi_render(std::ptr::null_mut(), std::ptr::null(), 0.0, 0.0, 0.0, 1.0),
                -1
            );
            assert_eq!(
                wgpu_ffi_render_offscreen(
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    1,
                    1,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0
                ),
                -1
            );
            assert_eq!(
                wgpu_ffi_read_pixels(std::ptr::null_mut(), std::ptr::null_mut(), 0),
                0
            );
        }
    }
}
