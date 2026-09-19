# Godot-style migration

Status: **Stage 25 — planning approved, implementation not started.**

Goal: turn quill from "a UI toolkit that also has a scene tree" into a
**2D-first scene engine** modeled on Godot, where a single `SceneTree` owns both
world (`Node2D`) and UI (`Control`), the camera drives the world only, and UI
lives under a `CanvasLayer` in viewport coordinates.

This document is the authoritative plan for the migration. `docs/plan.md` tracks
day-to-day work items; this file tracks the architecture change. When they
disagree, this file wins for Stages 25+.

## Why (current limits)

The current "two layers" is not caused by `Ui` being an instance, nor by the
native demo's `ControlFlow::Wait`. It is caused by structural decisions:

| # | Hard limit | Evidence |
|---|---|---|
| H1 | Two trees / two ownerships | `Ui` owns its own `SceneTree` (`crates/draw_ui/src/ui/mod.rs`), separate from any game tree |
| H2 | Layout in absolute viewport coords, detached from `CanvasItem.world_transform` | `ControlData::rect` is "absolute in logical viewport coordinates"; `Ui::paint` emits no `SetTransform` |
| H3 | Nodes have no user data / lifecycle | `draw_scene::Node` fields are fixed; `NodeKind` is a closed enum; AGENTS rule 5 bans ECS |
| H4 | Paint traversal and input routing are split | `SceneTree::paint` vs `Ui::paint`; `Ui::handle_input` vs game input |
| H5 | Dependency direction + frozen core | `draw_ui -> draw_scene`; rule 8 freezes `draw_ui::Widget` and the backend-neutral core |

`ControlFlow::Wait` (only in `demos/wgpu_demo`) and the WASM `requestAnimationFrame`
loop are **not** hard limits; the WASM runner already refreshes every frame.

## Target architecture

```
SceneTree
└─ RootViewport                      ← size, canvas_transform (camera), input routing
   ├─ World            (default canvas, layer=0)   ← affected by Camera2D
   │   ├─ Node2D / Sprite2D / ...
   │   └─ Camera2D
   └─ CanvasLayer      (layer=1, own transform)     ← ignores the camera
       └─ Control (UI, viewport-coordinate layout)
```

Key Godot semantics we adopt:

- `CanvasLayer` is a `Node` (not `Node2D`) that opens a new canvas transform
  context; the default world is the implicit layer `0`.
- UI is a normal subtree under a `CanvasLayer`; it shares the one `SceneTree`.
- `Camera2D` writes the viewport's `canvas_transform`; nodes outside any
  `CanvasLayer` are transformed by it, nodes inside a `CanvasLayer` are not.

## Prohibited / allowed

See `AGENTS.md` for the amended rules. In short, during the migration window:

- The core crates (`draw_scene`, `draw_ui`) **may** be changed incompatibly.
- Each stage still ends with the per-stage gate and a report, then waits for
  approval (rule 6 is unchanged).
- We do **not** import Godot, its editor, or an ECS. We stay backend-neutral.

## Phases

Each phase is a stage, gets its own report, and stops for approval.

### Phase 1 — `draw_scene` generalization (single tree + node extension point)

Purpose: kill the reason `Ui` became a god object — nodes need somewhere to hang
engine data.

- Add a backend-neutral extension slot to `Node` (`Box<dyn Any>` or a controlled
  `NodeData`). No ECS.
- Extend `NodeKind` with `CanvasLayer` and `Camera2D` (`Viewport` naming TBD).
- Give `SceneTree` the canvas-layer concept: every canvas item resolves the
  layer it belongs to.
- `SceneTree::paint` keeps painting only world `Visual`.
- Exit: headless tests green; old `Visual` API still works.

Open design point: `Node` is currently `Clone`. A `Box<dyn Any>` needs a clone
strategy (`Rc`/`Arc`) or dropping `Clone`. Decide before coding.

### Phase 2 — `Viewport` + `Camera2D` + view transforms

- Logical `Viewport`: `size`, `canvas_transform`, input-routing entry.
- `Camera2D`: `current`, `zoom`, `offset`, `anchor_mode` (limits / smoothing
  later).
- `SceneTree` computes `canvas_transform` (inverse of the camera world
  transform) during update.
- Helpers: `world_to_screen` / `screen_to_world` / `viewport_transform`
  (Godot `CanvasItem::get_global_transform_with_canvas`).
- `SceneTree::paint` emits `Save -> SetTransform(canvas_transform * world) -> ...`.
- Exit: headless tests for follow, zoom, coordinate round-trips.

### Phase 3 — `CanvasLayer` + layered painting

- `CanvasLayer { layer: i32, transform: Transform2D, follow_viewport: bool }`.
- On traversal, pick the effective transform per item: default canvas uses
  `viewport.canvas_transform`; a `CanvasLayer` subtree uses the layer transform
  composed with the node's local chain.
- `SceneTree::paint` emits layers in ascending `layer`, grouped by
  `Save/Restore + SetTransform`.
- Exit: moving the camera changes world commands but **not** the UI command
  sequence (golden test).

### Phase 4 — `Control` into the single tree + viewport-coordinate layout

This is the largest refactor; split it.

- **4a** — `draw_ui` stops owning a `SceneTree`; it borrows one.
  ```rust
  impl Ui {
      pub fn mount(&mut self, tree: &mut SceneTree, parent: NodeId, view: impl View);
      pub fn layout(&mut self, tree: &SceneTree, layer_rect: Rect); // viewport coords
      pub fn paint(&self, tree: &SceneTree, ctx: &mut PaintContext);
      pub fn handle_input(&mut self, tree: &SceneTree, ev: &InputEvent) -> EventResult;
  }
  ```
  `Ui` keeps only the environment: `Theme`, `TextMeasurer`, hover/pressed/focus,
  caches.
- **4b** — move `ControlData` onto the `Node` extension slot; keep
  `draw_components` / `ViewExt` / `Overlays` compiling with the new signatures.
- **4c** — migrate `demo_app`, `web_demo`, `component_demo`, `wgpu_demo`.
- Retain a compatibility layer (`Ui::new()` owning a tree) during 4a-4c.
- Exit: existing `draw_ui` / `demo_app` tests pass under the new signatures; UI
  and `Node2D` coexist in one tree.

### Phase 5 — unified lifecycle and input routing

- `SceneTree::process(dt)` dispatches per-node `process(dt)`.
- Input routing (order to be confirmed against Godot source):
  capture (`_input`) -> Node2D pick (`_input_event`) -> GUI pick
  (`Control::_gui_input`, canvas layer -> z -> tree order) -> `_unhandled_input`.
- Input completion: wheel, held key/button state, focus, hover, multi-touch,
  gamepad (later).
- Exit: headless tests for cross-layer picking, focus, handled propagation.

### Phase 6 — game capabilities (new crate `draw_game`)

Additive, outside the frozen core where possible.

- Sprites/textures: `Sprite2D` (or `Visual::Image`) with atlas / animation /
  flip / 9-slice; texture registration already exists in the backends.
- Primitives: `Line` / `Path` / `Arc` / `Ellipse` in all three backends +
  inspector.
- Collision: basic AABB / circle queries and `Area` triggers first; rigid bodies
  later (needs rule 5 relaxed — done in this stage).
- Timers / tweens / lightweight signals.
- Assets: image decode / texture loading pipeline.
- Audio: separate crate + backend.

### Phase 7 — native continuous loop + fixed timestep

- `demos/wgpu_demo`: `ControlFlow::Wait` -> `Poll` or `WaitUntil` fixed step.
- Separate logic step from render interpolation (`_physics_process` vs
  `_process`).
- `SubViewport` / offscreen render targets last.

### Phase 8 — observability / tests / docs

- `draw_profile`: canvas-layer and camera-matrix counters / audits.
- Tests: camera golden, layer order, input pick order, world/screen transforms.
- Update `architecture.md`, `backend.md`, `components.md`, `plan.md`,
  `AGENTS.md`.

## Dependency order

```
Phase1 -> Phase2 -> Phase3 -> Phase4 -> Phase5 -> Phase6 -> Phase7
                                              \-> Phase8 (continuous)
```

MVP = Phase 1 -> 2 -> 3 -> 4 (world + camera + CanvasLayer UI running).
Phase 5-6 are the second batch.

## Decisions (recommended defaults, pending sign-off)

1. **`Viewport` naming.** `draw_core::Viewport` is only a size + DPR helper.
   Rename it to `draw_core::ViewportSize` and give the new scene-level
   render-context node the name `Viewport` (root instance `RootViewport`).
2. **Tree access from `Ui`.** Pass `&mut SceneTree` explicitly to
   `mount` / `layout` / `paint` / `handle_input`. Prefer explicit borrowing over
   `Rc<RefCell<SceneTree>>`.
3. **Migration strategy.** Keep a `Ui::new()` compatibility layer (owns a tree)
   while adding the borrowed API; migrate demos afterwards.

## Open questions for Godot source review

Do not guess these; confirm from the Godot tree (downloaded by the user).

| # | Question | Godot source |
|---|---|---|
| Q1 | Does a `CanvasLayer` fully ignore `Viewport.canvas_transform`, or compose with it? Formula for `follow_viewport_enabled` / `follow_viewport_scale`? | `scene/main/canvas_layer.cpp`, `scene/main/viewport.cpp` (`_update_canvas_items`, `get_canvas_transform`, `get_final_transform`), `scene/2d/canvas_item.cpp` (`get_global_transform_with_canvas`, `get_viewport_transform`, `get_canvas_transform`), `servers/rendering/renderer_canvas_cull.cpp` |
| Q2 | Boundary between the logical `Viewport`, `Window` root viewport, and `SubViewport`; do we need `SubViewport` now? | `scene/main/viewport.h/.cpp`, `scene/main/window.cpp` |
| Q3 | `Control` parent-anchorable-rect rules; behavior when a `Control` sits under a `Node2D` / `CanvasLayer`; is `top_level` required for layer UI? | `scene/gui/control.cpp` (`get_parent_anchorable_rect`, `_size_changed`, `get_anchor`), `scene/2d/canvas_item.cpp` (`set_as_top_level`) |
| Q4 | GUI pick order across `CanvasLayer` (layer vs `z_index` vs tree order vs `mouse_filter`); ordering of `_input` / `_gui_input` / `_unhandled_input`. | `scene/main/viewport.cpp` (`_gui_find_control`, `_gui_input_event`, `_push_unhandled_input`), `scene/gui/control.cpp` (`_gui_input`) |
| Q5 | How `Camera2D` writes the canvas transform (`anchor_mode`, `zoom`, `offset`, limits, smoothing) and when `current` takes effect. | `scene/2d/camera_2d.cpp` |
| Q6 | Are `Control` and `Node2D` mixed under one parent allowed? How do `Container`s treat non-`Control` children? Must a UI subtree be pure `Control`? | `scene/gui/container.cpp`, `scene/gui/control.cpp`, `scene/2d/node_2d.cpp` |
