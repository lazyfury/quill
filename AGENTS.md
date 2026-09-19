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
draw_scene    -> draw_core
draw_ui       -> draw_core, draw_scene
draw_render   -> draw_core
draw_backend_* -> draw_render
draw_wasm     -> draw_render, draw_backend_canvas
```

Browser APIs only allowed in `draw_backend_canvas`, `draw_wasm`, `demos/web_demo`.

## Stages

- [x] Stage 0 — workspace skeleton
- [x] Stage 1 — core types / math
- [x] Stage 2 — SceneTree / Node / CanvasItem
- [ ] Stage 3 — DrawList / render IR
- [ ] Stage 4 — RecordingBackend / headless tests
- [ ] Stage 5 — Canvas2D backend + WASM
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
