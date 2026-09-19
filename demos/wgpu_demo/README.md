# wgpu demo

Native window demo for the `draw_backend_wgpu` backend.

It opens a `winit` window, renders the same `SceneTree` + `Ui` the web demos
use through `WgpuBackend`, and presents the result to a `wgpu` surface:

```text
winit events -> InputEvent -> Scene/UI -> DrawList -> WgpuBackend -> surface
```

No core, scene, UI or IR code changes between the Canvas backend and this one.

## Run

```bash
cargo run -p wgpu_demo --release
```

## Controls

- The rectangle in the scene rotates continuously.
- Click the **Click me** button in the right-hand panel; the status label
  updates the click count.
- Resize the window (the UI re-lays out) or move it between displays with
  different DPRs.
- Press `` ` `` (backtick) to toggle the **performance / debug overlay** in the
  top-right corner.

## Performance / debug overlay

The demo instruments each pipeline phase, records it in a `draw_profile::Profiler`,
audits the frame's `DrawList` with `draw_profile::inspect`, and renders the
result with `draw_debug_ui::DebugOverlay`:

```text
update/layout/paint/render (timed) -> Profiler.record -> inspect -> DebugOverlay -> DrawList
```

The panel shows FPS, current frame time (avg/max), per-phase averages, command
counts (current/peak), node & control counts, and inspection findings. Overlay
input is consumed over the panel; everything else is forwarded to the demo. See
`docs/debug.md` for wiring it into your own app.

## Notes

- Rendering is animated with `winit`'s `ControlFlow::Poll` and
  `Window::request_redraw`; no timer thread is used.
- The surface format prefers a non-sRGB format so shader output matches the
  Canvas backend; if only sRGB is offered, that is used as a fallback.
- The backend is window-agnostic: it receives a surface texture view via
  `WgpuBackend::begin_frame_with_view` and the demo calls `present()`. The same
  backend runs headlessly in `crates/draw_backend_wgpu/tests/render.rs`.
