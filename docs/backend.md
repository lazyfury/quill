# Backends

A `RenderBackend` consumes a backend-neutral `DrawList` and produces output. The
frame lifecycle is:

```text
begin_frame(Viewport) -> submit(&DrawList) (0..n) -> end_frame()
```

## Canvas 2D (`draw_backend_canvas`)

`Canvas2dBackend` wraps a `web_sys::CanvasRenderingContext2d` and maps each
`DrawCommand` to a Canvas API call.

### Coordinate handling

- Core/IR geometry is always in **logical pixels**.
- On `begin_frame` the canvas backing store is resized to
  `logical_size * scale_factor` (`devicePixelRatio`).
- Every `SetTransform` is multiplied by `scale_factor` before being applied, so
  DPR never enters the core or the IR.
- `ClipRect` is in **viewport/logical space**: the backend builds the clip path
  with a device-space transform, then reapplies the current logical transform
  (Canvas keeps the clip region in device space).

### Command mapping

| `DrawCommand` | Canvas 2D |
|---|---|
| `Save` / `Restore` | `ctx.save()` / `ctx.restore()` |
| `SetTransform` | `ctx.setTransform(...)` scaled by DPR |
| `SetOpacity` | `ctx.globalAlpha` |
| `ClipRect` | `beginPath` + `rect` + `clip` |
| `FillRect` / `StrokeRect` | `fillRect` / `strokeRect` |
| `FillCircle` / `StrokeCircle` | `arc` + `fill` / `stroke` |
| `DrawImage` | `drawImage` with a registered `TextureId` |
| `DrawText` | `font` + `textAlign` + `fillText` |

`TextureId` is resolved through `Canvas2dBackend::register_image`. No backend
object ever appears in `draw_core` / `draw_scene` / `draw_ui` / `draw_render`.

## Recording (`draw_backend_recording`)

Records each frame's viewport and concatenated commands. Used for the headless
test pipeline and `CommandAsserts`. This is the second backend used to validate
that the same `DrawList` drives a different `RenderBackend` without any
Scene/UI changes.

## wgpu (`draw_backend_wgpu`)

`WgpuBackend` consumes the same `DrawList` and rasterizes it with `wgpu` into an
offscreen `Rgba8Unorm` texture. `WgpuBackend::read_pixels` copies that texture
into a mapped buffer and returns tightly packed RGBA8, so the whole
`DrawList -> pixels` path is verifiable under native `cargo test` with no window
and no screenshot.

### How commands map onto the GPU

Transform, opacity and clip are resolved on the **CPU** while tessellating, so
the GPU pass is a single textured-triangle pipeline (`src/shader.wgsl`):

- `Save` / `Restore` push/pop a CPU state stack (transform, opacity, clip).
- `SetTransform` is applied while converting each vertex to NDC; `SetOpacity` is
  folded into the vertex color's alpha.
- `ClipRect` (viewport/logical space) becomes a per-draw scissor rectangle,
  converted to device pixels and clamped to the target.
- `FillRect` / `StrokeRect` / `FillCircle` / `StrokeCircle` tessellate into
  triangles and sample a 1x1 white texture.
- `DrawImage` samples a texture registered with `WgpuBackend::register_texture`
  (destination and optional source sub-rect map to UVs).
- `DrawText` samples a built-in `8x8` ASCII bitmap-font atlas generated at
  startup from the public-domain `font8x8` glyphs.

`set_scale_factor` scales the offscreen target to `logical * scale` and scales
vertex positions, so DPR never enters core or the IR — exactly like the Canvas
backend. The window runner converts winit's physical cursor position back to
logical pixels before feeding `InputEvent`s to the UI.

### Demo

`demos/wgpu_demo` is a `winit` runner: it installs the same `SceneTree` + `Ui`
composition as `demos/component_demo`, maps window events to `draw_core`
`InputEvent`s, renders with `WgpuBackend::begin_frame_with_view`, and presents
the surface. The backend stays the only `wgpu` renderer; the demo only drives
the window and the surface lifecycle.

### Scope

The default target is offscreen; `read_pixels` requires an offscreen frame. To
present in a window, pass the surface texture view to
`WgpuBackend::begin_frame_with_view` (the demo does this) and call
`SurfaceTexture::present` afterwards. The backend never creates a window or a
`wgpu::Surface` itself, so it stays window-agnostic.

### Pixel verification

`crates/draw_backend_wgpu/tests/render.rs` renders each command and asserts on
read-back pixels: solid fills, clear color, DPR scaling, clipping, opacity,
save/restore, baked transforms, circles, stroked rectangles, images, text, and a
full `SceneTree -> DrawList -> WgpuBackend` pipeline. Tests skip (rather than
fail) when no GPU adapter is available.

## Adding a new backend

1. Depend on `draw_render` (+ `draw_core` for shared types) only.
2. Implement `RenderBackend` for your type.
3. Do not modify Scene/UI: the same `DrawList` must drive the new backend.

## Where browser-specific code lives

Only `draw_backend_canvas`, `draw_wasm`, and `demos/web_demo` may reference
`web-sys` / `wasm-bindgen` / DOM APIs (and only under
`cfg(target_arch = "wasm32")`, so native `cargo test` stays headless).

`draw_backend_wgpu` uses no browser APIs; it is a native crate and depends only
on `draw_render` + `draw_core` (plus `wgpu`/`bytemuck`/`pollster`/`font8x8`).
