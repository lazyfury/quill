# quill

A backend-neutral 2D/UI drawing core in Rust, with HTML Canvas 2D (WASM) as the
first render backend. Inspired by Godot's `SceneTree -> Node -> CanvasItem ->
Node2D / Control` model, but with a Rust-friendly API.

## Architecture

```
Input -> SceneTree -> Update -> Layout -> Paint -> DrawList -> RenderBackend -> Pixels
```

| Crate | Responsibility |
|---|---|
| `draw_core` | math, color, IDs, base types |
| `draw_scene` | `Node`, `SceneTree`, `CanvasItem`, `Node2D`, transforms |
| `draw_ui` | `Control`, layout, containers, UI behavior |
| `draw_render` | `DrawCommand`, `DrawList`, `PaintContext`, `RenderBackend` |
| `draw_backend_canvas` | Canvas 2D backend |
| `draw_backend_recording` | headless recording backend for tests |
| `draw_wasm` | browser glue (events, RAF, canvas wiring) |

Dependency direction is enforced by crate boundaries: the pure core crates never
depend on browser APIs or a concrete backend. See `AGENTS.md`.

## Status

Stage 4 (RecordingBackend / headless tests). `draw_core` provides math, colors, handles and
the viewport model; `draw_scene` provides the scene tree with transform/visibility
propagation and a `SceneTree::paint` step; `draw_render` provides the
backend-neutral IR (`DrawCommand`/`DrawList`/`PaintContext`) and the
`RenderBackend` trait; `draw_backend_recording` records frames for the fully
headless `Scene -> DrawList -> RenderBackend` test pipeline. The Canvas/WASM
backend arrives in Stage 5.

## Build & test

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
```

Browser/WASM build and the web demo arrive in Stage 5.
