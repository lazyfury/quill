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

## Adding a new backend

1. Depend on `draw_render` (+ `draw_core` for shared types) only.
2. Implement `RenderBackend` for your type.
3. Do not modify Scene/UI: the same `DrawList` must drive the new backend.

## Where browser-specific code lives

Only `draw_backend_canvas`, `draw_wasm`, and `demos/web_demo` may reference
`web-sys` / `wasm-bindgen` / DOM APIs (and only under
`cfg(target_arch = "wasm32")`, so native `cargo test` stays headless).
