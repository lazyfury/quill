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
(Detailed conventions are finalized in Stage 1.)

## Implementation stages

- Stage 0 — workspace skeleton
- Stage 1 — core types / math (`Vec2`, `Rect`, `Transform2D`, `Color`, `NodeId`)
- Stage 2 — `SceneTree` / `Node` / `CanvasItem` / `Node2D`
- Stage 3 — `DrawList` / render IR
- Stage 4 — `RecordingBackend` / headless pipeline
- Stage 5 — Canvas 2D backend + WASM
- Stage 6 — `Control` / layout / input
- Stage 7 — reusable component demo
- Stage 8 — second backend validation

## Backend replaceability

Same `Scene` + `UI` + `DrawList` must run on any backend without changing
Scene/UI code. Backend-specific code lives only in `draw_backend_*`,
`draw_wasm`, and the web demo.
