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

## Notes

- Rendering is animated with `winit`'s `ControlFlow::Poll` and
  `Window::request_redraw`; no timer thread is used.
- The surface format prefers a non-sRGB format so shader output matches the
  Canvas backend; if only sRGB is offered, that is used as a fallback.
- The backend is window-agnostic: it receives a surface texture view via
  `WgpuBackend::begin_frame_with_view` and the demo calls `present()`. The same
  backend runs headlessly in `crates/draw_backend_wgpu/tests/render.rs`.
