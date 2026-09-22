/*
 * wgpu_ffi.h — C ABI over `draw_backend_wgpu` (`wgpu_ffi`).
 *
 * The host owns the window and the DrawList; this library owns the wgpu
 * instance, surface, device and the Rust renderer. A null view creates a
 * headless (offscreen-only) renderer.
 *
 * On macOS the view is an `NSView*` (e.g. from GLFW's `glfwGetCocoaWindow` ->
 * `contentView`). The view must outlive the handle.
 *
 * Layout mirrors `examples/wgpu_ffi/src/lib.rs`. A null handle is a no-op.
 */
#ifndef WGPU_FFI_H
#define WGPU_FFI_H

#include <stddef.h>
#include <stdint.h>

#include "quill.h"

#ifdef __cplusplus
extern "C" {
#endif

/* An opaque, Rust-owned wgpu renderer. */
typedef struct WgpuFfi WgpuFfi;

/* Create the renderer. `ns_view` null => headless. Returns null on failure. */
WgpuFfi *wgpu_ffi_new(void *ns_view, uint32_t width, uint32_t height, float scale);
void wgpu_ffi_free(WgpuFfi *handle);

/* Reconfigure the surface for a new device-pixel size / scale factor. */
void wgpu_ffi_resize(WgpuFfi *handle, uint32_t width, uint32_t height, float scale);

/* Render `list` to the window surface and present it. 0 on success. */
int32_t wgpu_ffi_render(WgpuFfi *handle, const QuillDrawList *list, float clear_r,
                        float clear_g, float clear_b, float clear_a);

/* Render `list` into the backend's offscreen texture (device-pixel size). */
int32_t wgpu_ffi_render_offscreen(WgpuFfi *handle, const QuillDrawList *list, uint32_t width,
                                  uint32_t height, float scale, float clear_r, float clear_g,
                                  float clear_b, float clear_a);

/* Copy the last offscreen frame into `out` as tightly packed RGBA8; returns
 * bytes written. `out` must hold width * height * 4 bytes. */
size_t wgpu_ffi_read_pixels(WgpuFfi *handle, uint8_t *out, size_t out_len);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* WGPU_FFI_H */
