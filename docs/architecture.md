# Architecture

## Pipeline

```
Input -> SceneTree -> Update -> Layout -> Paint -> DrawList -> RenderBackend -> Pixels
```

## Layers

- **Scene / UI** — tree structure, state, layout, events, draw intent.
- **DrawList** — backend-neutral intermediate representation (IR).
- **RenderBackend** — turns the IR into concrete output (Canvas, recording, ...).

## Execution model (planned)

1. **Input** — events are dispatched to nodes (target dispatch in MVP; capture/
   bubble is a future extension point).
2. **Update** — application/component state is mutated; dirty flags are set.
3. **Layout** — `Control` anchors/offsets/containers resolve sizes and positions.
4. **Paint** — visible nodes emit `Paint` calls into a `DrawList`.
5. **Render** — a `RenderBackend` consumes the `DrawList` and produces output.

Only steps 4-5 cross the core/backend boundary, and they cross via `DrawList`.

## Coordinates

Logical pixels are the core unit. Browser device pixel ratio (DPR) is handled
only at the backend/WASM edge and never enters core business logic.

Finalized conventions (Stage 1): origin top-left, `+X` right, `+Y` down,
rotations in radians (positive from `+X` toward `+Y`), rectangles axis-aligned
with half-open membership `[min, max)`. `Viewport` stores logical size only;
`Viewport::device_size(scale)` derives device pixels without storing DPR.

## Implementation stages

- Stage 0 — workspace skeleton
- Stage 1 — core types / math (`Vec2`, `Rect`, `Transform2D`, `Color`, `NodeId`) [done]
- Stage 2 — `SceneTree` / `Node` / `CanvasItem` / `Node2D` [done]
- Stage 3 — `DrawList` / render IR [done]
- Stage 4 — `RecordingBackend` / headless pipeline [done]
- Stage 5 — Canvas 2D backend + WASM [done]
- Stage 6 — `Control` / layout / input [done]
- Stage 7 — reusable component demo [done]
- Stage 8 — second backend validation (macOS Core Graphics, `draw_backend_coregraphics`) [done]

## Backend replaceability

Same `Scene` + `UI` + `DrawList` must run on any backend without changing
Scene/UI code. Backend-specific code lives only in `draw_backend_*`,
`draw_wasm`, and the demos.

Validated by two independent renderers consuming the same IR:

- `draw_backend_canvas` (HTML Canvas 2D, WASM) — `demos/web_demo`,
  `demos/component_demo`.
- `draw_backend_coregraphics` (macOS Core Graphics + Core Text) —
  `demos/macos_demo`.

Reused unchanged by both: `draw_core`, `draw_scene`, `draw_ui`, and the
`DrawList` / `RenderBackend` contract in `draw_render`.
Backend-specific: command-to-API mapping, resource registration, and the
platform loop/window (`draw_wasm`, the demos).
