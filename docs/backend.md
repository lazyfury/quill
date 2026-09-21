# Backends

A `RenderBackend` consumes a backend-neutral `DrawList` and produces output. The
frame lifecycle is:

```text
begin_frame(ViewportSize) -> submit(&DrawList) (0..n) -> end_frame()
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
| `Line` | `beginPath` + `moveTo` + `lineTo` + `stroke` |
| `FillCircle` / `StrokeCircle` | `arc` + `fill` / `stroke` |
| `DrawImage` | `drawImage` with a registered `TextureId` |
| `DrawText` | `font` + `textAlign` + `fillText` |

`TextureId` is resolved through `Canvas2dBackend::register_image`. No backend
object ever appears in `draw_core` / `draw_scene` / `draw_ui` / `draw_render`.

`DrawText` positions are baselines. `draw_backend_canvas::font_spec` is the
single font spec the backend draws with; `draw_wasm::CanvasTextMeasurer` measures
with the same spec (`measureText`, whole runs for shaping/kerning) and is
injected via `draw_ui::set_text_measurer`, so layout ascents, run widths and
painted baselines agree.

The runner also reflects hover feedback: `App::pointer_cursor` (usually
`draw_ui::hovered_is_button` or `draw_ui::is_interactive`) drives the canvas
CSS `cursor` property (`pointer` / `default`).

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
- `FillRect` / `StrokeRect` / `Line` / `FillCircle` / `StrokeCircle` tessellate
  into triangles and sample a 1x1 white texture. `Line` becomes a thin quad
  with butt caps.
- The render pass is **4x multisampled**: geometry is drawn into an MSAA texture
  and resolved into the frame's texture each frame (offscreen and surface alike).
  Combined with 64-segment circles, 8-segment rounded-rect corners and the SVG
  vector resolution (`docs/svg.md`), edges are anti-aliased instead of hard.
- `DrawImage` samples a texture registered with `WgpuBackend::register_texture`
  (destination and optional source sub-rect map to UVs). A host that changes an
  image repeatedly (a painting canvas, a live preview) should call
  `WgpuBackend::update_texture` instead: when the size is unchanged it rewrites
  the existing GPU texture and keeps its bind group, so the per-update cost is
  just the pixel copy rather than a fresh texture + view + bind group. Sampling
  is per texture: `WgpuBackend::set_texture_filter` (or
  `register_texture_with_filter`) chooses `TextureFilter::{Linear, Nearest}` —
  the default is `Linear`, and `Nearest` keeps texel edges for pixel art / a
  zoomed low-resolution canvas. The filter is a backend-side property of the
  `TextureId`; the neutral `DrawImage` command stays filter-free.
- `DrawText` uses a real font loaded at startup with `ab_glyph` (`QUILL_FONT`
  if set, otherwise a per-OS candidate list: macOS `Arial Unicode`, Linux
  `DejaVuSans`/Noto CJK, Windows Arial/MSYH) and shaped with `rustybuzz` plus
  `unicode-bidi`. Bidi runs are reordered into visual order; each run is shaped
  so kerning, ligatures and contextual forms apply; glyphs are rasterized by
  glyph id on demand at the requested size into a `1024x1024` shelf-packed
  atlas uploaded to the GPU after each `submit`. UVs, shaped advances and
  baselines come from the font. `WgpuBackend::text_metrics()` exposes the same
  metrics as a `FontMetrics` so hosts can build a matching
  `draw_ui::TextMeasurer` (whose `measure_run` sums shaped advances).
- `FontConfig` chooses the look: `FontMode::System` (default) or
  `FontMode::Pixel` (the built-in bitmap), plus
  `device_pixel_rasterization` (default `true`) which rasterizes system glyphs
  at `font_size * scale` for crisp HiDPI text while keeping logical metrics.
  `WgpuBackend::set_font_config` rebuilds the atlas at runtime. In pixel mode the
  8x8 cell is drawn at `PIXEL_GLYPH_RATIO * font_size` (default 0.75, rounded to
  whole pixels; advances scale likewise) so it matches a proportional font's
  visual size instead of filling the whole em.
- If no font file loads, `System` mode falls back to the built-in `8x8` ASCII
  bitmap atlas (from the public-domain `font8x8`); unsupported characters then
  sample a box-shaped "missing glyph" cell.

`set_scale_factor` scales the offscreen target to `logical * scale` and scales
vertex positions, so DPR never enters core or the IR — exactly like the Canvas
backend. The window runner converts winit's physical cursor position back to
logical pixels before feeding `InputEvent`s to the UI.

### Demo

`examples/wgpu_demo` is a `winit` runner around the shared backend-neutral
`examples/demo_app` app: it maps window events to `draw_core` `InputEvent`s, calls
`DemoApp::update/layout`, renders with `WgpuBackend::begin_frame_with_view`, and
presents the surface. It injects the backend's `FontMetrics` as a
`draw_ui::TextMeasurer` (shaping included). The backend stays the only `wgpu` renderer; the demo only drives the window
and the surface lifecycle. The same `DemoApp` runs under `examples/web_demo` on the
Canvas backend.

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

Only `draw_backend_canvas`, `draw_wasm`, and `examples/web_demo` may reference
`web-sys` / `wasm-bindgen` / DOM APIs (and only under
`cfg(target_arch = "wasm32")`, so native `cargo test` stays headless).

`draw_backend_wgpu` uses no browser APIs; it is a native crate and depends only
on `draw_render` + `draw_core` (plus `wgpu`/`bytemuck`/`pollster`/`font8x8`).
