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
- [ ] Stage 1 — core types / math
- [ ] Stage 2 — SceneTree / Node / CanvasItem
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
