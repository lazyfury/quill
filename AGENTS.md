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

## Dependency direction

```
draw_core            (no draw_* deps)
draw_scene    -> draw_core, draw_render
draw_ui       -> draw_core, draw_scene
draw_render   -> draw_core
draw_backend_* -> draw_render, draw_core
draw_wasm     -> draw_render, draw_backend_canvas, draw_core
web_demo      -> draw_core, draw_render, draw_scene, draw_wasm
```

`draw_scene -> draw_render` is intentional: `draw_render` is the backend-neutral
IR (no backend/browser deps), and the pipeline's Paint step (Scene -> DrawList)
lives in the scene. This does not weaken backend replaceability.

Browser APIs only allowed in `draw_backend_canvas`, `draw_wasm`, `demos/web_demo`.

## Stages

- [x] Stage 0 — workspace skeleton
- [x] Stage 1 — core types / math
- [x] Stage 2 — SceneTree / Node / CanvasItem
- [x] Stage 3 — DrawList / render IR
- [x] Stage 4 — RecordingBackend / headless tests
- [x] Stage 5 — Canvas2D backend + WASM
- [ ] Stage 6 — Control / layout / input
- [ ] Stage 7 — reusable component demo
- [ ] Stage 8 — second backend validation

## Per-stage gate (must run)

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

Then emit the fixed report format and stop for approval.

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
owns the RAF loop and `App::{update, paint}`. `ClipRect` is applied in device
space then the logical transform is reapplied. Build/run the demo with
`demos/web_demo/build.sh` + a static server. Only these two crates + the demo may
touch `web-sys`/browser APIs.
