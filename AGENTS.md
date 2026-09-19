# AGENTS.md — quill

Concise working agreement for agents. The full spec lives in `docs/architecture.md`
(stage plan) — but this file is the short source of truth.

## Goal

Backend-neutral 2D/UI drawing core in Rust. Canvas 2D via WASM is the first
backend. Godot-inspired: `SceneTree -> Node -> CanvasItem -> Node2D / Control`.

## Pipeline (must hold)

```
Input -> SceneTree -> Update -> Layout -> Paint -> DrawList -> RenderBackend -> Pixels
```

## Hard rules

1. `draw_core`, `draw_scene`, `draw_ui`, `draw_render` MUST NOT depend on
   `web_sys` / `wasm_bindgen` / DOM / `wgpu` / any concrete backend.
2. `DrawCommand` holds only backend-neutral data (no Canvas/WebGL/WGPU objects).
3. Scene/UI must be testable with native `cargo test`, no browser.
4. Resources use handles (`NodeId`, `TextureId`), not backend objects.
5. No ECS, shaders, render graph, particles, physics, editor in MVP.
6. Do not merge stages. Each stage ends with a report and waits for user approval.
7. **No screenshot / screen-recording visual testing.** Never use
   `screencapture`, browser screenshots, screen recording, or any OS-level
   capture to verify rendering. Verify programmatically instead: read the
   backend's own pixel buffer, assert `DrawList` command sequences, or read DOM
   state markers. If a claim cannot be verified without a screenshot, say so
   rather than capturing one.

## Dependency direction

```
draw_core            (no draw_* deps)
draw_scene    -> draw_core, draw_render
draw_ui       -> draw_core, draw_scene, draw_render
draw_render   -> draw_core
draw_profile  -> draw_core, draw_render
draw_debug_ui -> draw_core, draw_render, draw_ui, draw_profile
draw_backend_* -> draw_render, draw_core
draw_wasm     -> draw_render, draw_backend_canvas, draw_core
web_demo      -> draw_core, draw_render, draw_scene, draw_wasm
wgpu_demo     -> draw_core, draw_render, draw_scene, draw_ui, draw_backend_wgpu,
                 draw_profile, draw_debug_ui, winit
```

`draw_scene -> draw_render` is intentional: `draw_render` is the backend-neutral
IR (no backend/browser deps), and the pipeline's Paint step (Scene -> DrawList)
lives in the scene. This does not weaken backend replaceability.

Browser APIs only allowed in `draw_backend_canvas`, `draw_wasm`, `demos/web_demo`.
The native window API (`winit`) is only allowed in `demos/wgpu_demo`.

## Stages

- [x] Stage 0 — workspace skeleton
- [x] Stage 1 — core types / math
- [x] Stage 2 — SceneTree / Node / CanvasItem
- [x] Stage 3 — DrawList / render IR
- [x] Stage 4 — RecordingBackend / headless tests
- [x] Stage 5 — Canvas2D backend + WASM
- [x] Stage 6 — Control / layout / input
- [x] Stage 7 — reusable component demo
- [x] Stage 8 — second backend validation (required case covered by
      `draw_backend_recording`; an extra native backend was tried and removed)
- [x] Stage 9 — wgpu backend (`draw_backend_wgpu`, offscreen + pixel readback)
- [x] Stage 10 — performance inspection (`draw_profile`) + debug overlay
      (`draw_debug_ui`)

## Per-stage gate (must run)

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

Then emit the fixed report format and stop for approval.

## Second backend (Stage 8)

The required second-backend validation is satisfied by `draw_backend_recording`
(a different `RenderBackend` consuming the same `DrawList`, no Scene/UI changes).

A native macOS Core Graphics backend plus a `macos_demo` was implemented and then
**removed by request**: the result was judged not worth the added complexity. Do
not reintroduce it without an explicit request.

## wgpu backend (Stage 9)

`draw_backend_wgpu` is a third `RenderBackend` over the same `DrawList`. It
renders to an **offscreen** `Rgba8Unorm` texture by default and exposes
`WgpuBackend::read_pixels()` so tests assert on real pixels under native
`cargo test` (no window, no screenshot). It can also draw into an external view
(window surface texture) via `begin_frame_with_view`; `demos/wgpu_demo` presents
that with `winit`. Transform/opacity/clip are resolved on the CPU; one
textured-triangle pipeline handles solid shapes, registered images, and a
built-in `8x8` bitmap-font atlas. `wgpu` must stay confined to this crate plus
its own tests and the demo; core/scene/UI/render never see it.

## Performance inspection (Stage 10, `draw_profile` + `draw_debug_ui`)

`draw_profile` is a backend-neutral observer of the pipeline. It depends only on
`draw_core` + `draw_render` and never measures time itself: the host samples
`Instant` per phase and feeds milliseconds in, so the model is deterministic and
unit-testable.

- `FrameStats` = `index`, `frame_ms`, `StageTimes` (`update`/`layout`/`paint`/
  `render`) and `FrameCounters` (`scene_nodes`, `controls`, `draw_commands`,
  `draw_lists`).
- `Profiler` keeps a bounded ring buffer of `FrameStats`, can be disabled at
  runtime (no-op hot path), and derives a `FrameSummary` (avg/min/max, per-phase
  averages, max commands, FPS).
- `inspect(&DrawList, &FrameStats) -> InspectionReport` audits the frame with
  `Severity`-ranked `Finding`s, aggregated by `FindingCode`: save/restore
  balance, non-finite geometry/transform, degenerate rect/circle/stroke/clip,
  opacity range, empty text, and command/frame-time/entity budgets
  (`InspectionConfig`, default 2048 commands / 16.7 ms / 10k entities).

`draw_debug_ui::DebugOverlay` turns a `Profiler` + `InspectionReport` into an
ordinary `draw_ui` panel (its own `Ui` tree, painted after the app UI). It is
toggled by the host and is a no-op while closed. `demos/wgpu_demo` instruments
its frame, runs `inspect`, and toggles the overlay with the backtick key.

Like all core crates these two are verified with native `cargo test`; the window
overlay itself is not screenshot-verified (see `docs/testing.md`).

## API priority: API -> test -> implementation -> integration.

## Core types (Stage 1, `draw_core`)

`Vec2`, `Size`, `Edges`, `Rect`, `Transform2D`, `Color`, `NodeId` +
`NodeIdAllocator`, `Viewport`.
Conventions: origin top-left, +X right, +Y down, logical pixels, radians,
positive rotation +X -> +Y. Rect membership is half-open `[min, max)`.
DPR never enters core: `Viewport::device_size(scale)` is a pure helper.

## Scene (Stage 2, `draw_scene`)

`SceneTree` arena over `Node` + `NodeId`. `NodeKind::{Node, Node2D}`; `Node2D`
owns a `CanvasItem` (local transform, visibility, z-index). `SceneTree::update()`
derives `world_transform` / `world_visible` using `DirtyFlags` (returns number of
recomputed transforms; 0 when clean). Child lists are kept sorted by
`(z_index, creation order)` for deterministic traversal. Transform propagation:
`world = parent_world * local`.

## Render IR (Stage 3, `draw_render`)

`Paint` (solid color), `DrawCommand` (`Save`/`Restore`/`SetTransform`/
`SetOpacity`/`ClipRect`/`FillRect`/`StrokeRect`/`FillCircle`/`StrokeCircle`/
`DrawImage`/`DrawText`), `DrawList`, `PaintContext`, `TextureId`.
`PaintContext::save`/`restore` are balanced. Scene paints via
`SceneTree::paint(&mut PaintContext)`; `Visual::{None,Rect,Circle}` on `Node2D`
are a temporary built-in primitive. Geometry is in current-transform space;
`ClipRect` is in viewport/logical space. No backend types in the IR.

## Backend contract (Stage 4, `draw_render` + `draw_backend_recording`)

`RenderBackend` trait: `begin_frame(Viewport)` -> `submit(&DrawList)` (0..n) ->
`end_frame()`, with an associated `Error`. `RecordingBackend` records each
frame's viewport + concatenated commands. `CommandAsserts` gives
count/contains/sequence/last-transform/opacity/clip assertions. Full headless
pipeline test lives in `draw_backend_recording/tests/pipeline.rs`.

## Browser layer (Stage 5)

`Canvas2dBackend` maps `DrawCommand` to Canvas 2D. Logical coords are kept; the
backing store is `logical * scale_factor` and every transform is multiplied by
the scale factor, so DPR never reaches core/IR. `draw_wasm::start(canvas_id, app)`
owns the RAF loop and `App::{update, paint, event}`. `ClipRect` is applied in device
space then the logical transform is reapplied. Build/run the demo with
`demos/web_demo/build.sh` + a static server. Only these two crates + the demo may
touch `web-sys`/browser APIs.

## UI (Stage 6, `draw_ui`)

`Ui` owns a `SceneTree` of `Control` nodes plus `ControlData` (anchors/offsets/
min_size/rect/mouse_filter) and `Widget` (Panel/Label/Button/VBox/HBox).
`layout(viewport)` resolves absolute rects; `paint(ctx)` emits the DrawList;
`handle_input(&InputEvent)` does topmost hit testing + target dispatch (capture/
bubble reserved). Pointer position is computed from `clientX/Y` minus the canvas
bounding rect. `InputEvent`/`EventResult` live in `draw_core`. Browser click path
is verified in headless Chrome via `?selftest=1`.

## Component API (Stage 7, `draw_ui`)

`Component` trait + builder structs `Panel`/`VBox`/`HBox`/`Label`/`Button`.
`ui.add(parent, Button::new("x").on_click(..))` returns an owned `ControlRef`.
Demos: `demos/web_demo` (raw API) and `demos/component_demo` (recommended API).
