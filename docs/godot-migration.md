# Godot-style migration

Status: **Stage 25 accepted.** Phases 1-5 and sub-stages 25.1-25.16 landed;
Phases 6-9 (`draw_game`, native continuous loop, observability, `quill` facade)
are future stages, not part of Stage 25's acceptance. Post-25.16 work:
`draw_font` (system font service + numeric `FontWeight`) and `Theme` as a trait
+ `DefaultTheme`.

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

`ControlFlow::Wait` (only in `examples/wgpu_demo`) and the WASM `requestAnimationFrame`
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

### Phase 2 — `Viewport` + `Camera2D` + view transforms (DONE, Stage 25.2)

- Logical `Viewport`: `size`, `canvas_transform`, input-routing entry.
- `Camera2D`: `current`, `zoom`, `offset`, `anchor_mode` (limits / smoothing
  later).
- `SceneTree` computes `canvas_transform` (inverse of the camera world
  transform) during update.
- Helpers: `world_to_screen` / `screen_to_world` / `viewport_transform`
  (Godot `CanvasItem::get_global_transform_with_canvas`).
- `SceneTree::paint` emits `Save -> SetTransform(canvas_transform * world) -> ...`.
- Exit: headless tests for follow, zoom, coordinate round-trips.

Landed as `draw_scene::{Viewport, Camera2DData, AnchorMode}`, the tree root is
now `NodeKind::Viewport` (name `root`, Godot `RootViewport`), and
`draw_core::Viewport` was renamed to `draw_core::ViewportSize`. Camera math is a
direct port of the transform-relevant part of Godot
`Camera2D::get_camera_transform` (no limits / drag / rotation / smoothing):
`zoom_scale = 1/zoom`, `screen_offset = center ? size/2 * zoom_scale : 0`,
`canvas_transform = affine_inverse(scale(zoom_scale) with origin camera_pos -
 screen_offset + offset)`.

### Phase 3 — `CanvasLayer` + layered painting (DONE, Stage 25.3)

- `CanvasLayer { layer: i32, transform: Transform2D, follow_viewport: bool }`.
- On traversal, pick the effective transform per item: default canvas uses
  `viewport.canvas_transform`; a `CanvasLayer` subtree uses the layer transform
  composed with the node's local chain.
- `SceneTree::paint` emits layers in ascending `layer`, grouped by
  `Save/Restore + SetTransform`.
- Exit: moving the camera changes world commands but **not** the UI command
  sequence (golden test).

Landed: `SceneTree::canvas_transform_of` implements Godot
`CanvasItem::get_canvas_transform` (nearest `CanvasLayer` final transform, else
the root viewport transform); `canvas_layer_final_transform` implements Godot
`CanvasLayer::get_final_transform` (`follow_viewport` composes the camera, scale
not modeled yet); `viewport_transform` is layer-aware. `SceneTree::paint` now
batches visible canvas items into layer groups sorted ascending by `layer`
(stable for ties), emitting `Save -> SetTransform(group) -> [per item:
Save -> SetTransform(effective * world) -> draw -> Restore] -> Restore`. The
golden test `canvas_layer_ignores_camera` asserts moving the camera changes the
world SetTransforms but leaves the UI group's transforms identical.

### Phase 4 — `Control` into the single tree + viewport-coordinate layout

This is the largest refactor; split it.

- **4a** — `draw_ui` stops owning a `SceneTree`; it borrows one. **(DONE, Stage 25.4a)**
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

  Landed: `Ui` no longer owns a tree. It keeps the environment, the per-control
  layout data (`ControlData`/`Widget`), interaction state and caches, plus a
  cached `parent_of`/`children_of` index so property setters and interaction
  state stay tree-free. All build and traversal entry points take the tree:
  `add_*`/`insert`, `mount`, `layout`, `paint`, `paint_debug`, `hit_test`,
  `handle_input`. `Component::mount(self, ui, tree, parent)`, `BuildContext`
  carries `&mut Ui` + `&mut SceneTree`, and `ViewExt`/`Modify` thread both.
  
  Compatibility: `UiHost` owns a `SceneTree` + root control and re-exposes the
  old convenience API; `draw_components`' `Overlays`, `draw_debug_ui` and the
  demos use it (pending 4c). `Ui::layout` takes `&SceneTree` + `ViewportSize`
  (the tree identifies UI roots; the viewport rect is the layer rect) rather
  than `Rect`.

- **4b** — move `ControlData` onto the `Node` extension slot; keep
  `draw_components` / `ViewExt` / `Overlays` compiling with the new signatures.
  **(DONE, Stage 25.4b)**

  `ControlData` now lives in the node's generic `data` slot (`Node::set_data` /
  `data::<ControlData>`), the first real engine type to use the Phase 1
  extension point. `Ui::register_control` writes it on insert, `set_*` mutate it
  in place (`with_control`), and `control`/`paint`/`input` read it from the tree.
  `Ui` keeps only `widgets`/`callbacks`/`decorations` plus the `parent_of` /
  `children_of` structural index. `Ui::layout` takes `&mut SceneTree` (it writes
  resolved rects back) and uses a transient scratch copy of each control's data
  for the measure/arrange pass, then writes the result back to the slots.

  **Extended (Stage 25.6, Phase 4d):** the remaining per-node state moved onto
  the tree too. `ControlData` + `Widget` + click callback + decorations +
  `layout_dirty` now form one `draw_ui::Control` bundle stored in the node's
  extension slot; pointer hover/press/focus ownership lives in a `GuiState` on
  the root node. `Ui` retains only the environment (theme, text measurer) and
  layout caches (`order_cache`/`text_cache`/`measure_cache` + validity
  counters), with no per-node `HashMap`s, no interaction pointers and no dirty
  set. `draw_ui` still has no state that `draw_scene` would need to know about —
  the tree is the single source of truth.

  **Finalized (Stage 25.7, Phase 4e):** the layout cache and pass counters
  (`valid`/`viewport`/`count`/`last_arranged` + the order/text/measure maps)
  moved into a `LayoutCache` behind a `RefCell` inside a `UiRootState` on the
  root node. `Ui` is now exactly `{ theme, text_measurer }` — a pure
  environment. One `Ui` can therefore drive more than one tree, and the tree
  owns all UI state.

  **Zero-sized (Stage 25.8, Phase 4f):** `theme` and `text_measurer` joined
  `UiRootState` too, so `Ui` is now a zero-sized API handle (`pub struct Ui;`)
  and the root node owns *all* UI state. `ui.theme(tree)` /
  `ui.set_theme(tree, …)` and `ui.set_text_measurer(tree, …)` read and write the
  root state; layout/paint read the measurer from there. No globals/singletons
  are used, and one environment handle can drive several trees.

  **Free functions (Stage 25.9, Phase 4g):** the `Ui` / `UiHost` types were
  removed entirely. `draw_ui` now exposes free functions over the tree
  (`ui::add_label(&mut tree, …)`, `ui::layout(&mut tree, vp)`,
  `ui::paint(&tree, ctx)`, `ui::route_input(&mut tree, ev)`, …); `Component::mount`
  and `BuildContext` carry only the tree, and demos hold just a `SceneTree`.
  `docs/` examples use this style.

  **Component-native (Stage 25.10/25.11):** the `View`/`ViewExt`/`Modify`/
  `BuildContext` layer and the `add_*`/`mount`/`insert` free functions were
  deleted. `draw_scene` gained `SceneChild` + `SceneTree::add_child(parent, c)`,
  and `draw_components::Component` now carries a `Spec` and exposes the modifiers
  (`child`, `background`, `surface`, `dynamic_background`, `foreground`,
  `on_click`, `grow`, `min_size`, …) as methods. `draw_components` components
  take a `&'static dyn Theme`; **the theme is no longer stored on
  the tree** (Phase 4f is reversed for the theme only — the text measurer still
  lives on the root). `draw_ui` decorators no longer take a `Theme`; their
  closures capture the colors they need.
- **4c** — migrate `demo_app`, `web_demo`, `wgpu_demo` to the
  borrowed API (the demos currently use the `UiHost` compatibility host).
  **(DONE, Stage 25.4c)**

  `DemoApp` now owns a `SceneTree` + borrowed `Ui`; the `wgpu_demo` and
  `web_demo` hosts drive it through `ui()`/`tree()`. `Overlays::layout` took the
  borrowed host (`&Ui` + `&SceneTree`); `DebugOverlay::paint` takes `&Ui` +
  `&SceneTree`. `UiHost` remains available and is still used by the overlay
  layer's owned sub-UI and by the unit tests.
- Retain a compatibility layer (`UiHost`) during 4a-4c. **(in place)**
- Exit: existing `draw_ui` / `demo_app` tests pass under the new signatures; UI
  and `Node2D` coexist in one tree. **(met: `demo_app` migrated, borrowed-API
  test in `draw_ui`)**

### Phase 5 — unified lifecycle and input routing (DONE, Stage 25.5)

- `SceneTree::process(dt)` dispatches per-node `process(dt)`.
- Input routing (order to be confirmed against Godot source):
  capture (`_input`) -> Node2D pick (`_input_event`) -> GUI pick
  (`Control::_gui_input`, canvas layer -> z -> tree order) -> `_unhandled_input`.
- Input completion: wheel, held key/button state, focus, hover, multi-touch,
  gamepad (later).
- Exit: headless tests for cross-layer picking, focus, handled propagation.

Landed: `Node` gained `process` / `_input` / `_input_event` /
`_unhandled_input` callbacks with `SceneTree::set_*` and a `SceneTree::process(dt)`
tick. `SceneTree::handle_input` runs capture (tree order) then the world pick
(`pick_world`: topmost visible canvas item under the pointer, honoring camera
and `CanvasLayer` transforms, only nodes with an `_input_event` handler).
`Ui::route_input(tree, event)` chains `_input` -> world -> GUI ->
`_unhandled_input` (Godot `Viewport::push_input` order confirmed from source).
Routing is **owned by `draw_scene`**: `SceneTree::route_input` runs the full
order for UI-less games, and `SceneTree::route_input_with(&mut dyn GuiInput, …)`
inserts a GUI stage. `draw_ui::Ui` implements `draw_scene::GuiInput`, so the
engine has no dependency on UI and an app without a HUD never needs
`draw_ui`.
GUI drags use pointer capture (`GuiState.dragging` + `Control.drag_callback`):
on `PointerDown` a node with a drag callback captures the pointer, `PointerMove`
is routed to it as a delta (even outside its rect) and `PointerUp` releases it.
`draw_components::{set_on_drag}` and `Component::on_drag` expose it; `ResizeHandle`
uses it for split-view resizing.
`draw_core` gained `InputEvent::Wheel` and `InputState` (held buttons/keys +
pointer position). Multi-touch / gamepad remain future work; GUI focus/hover
were already in `Ui` and a cross-layer focus test was added.

### Phase 6 — game capabilities (new crate `draw_game`) — NOT STARTED

Additive, outside the frozen core where possible. (Only the `Visual::Image` and
`Line` items below have landed early, in Stage 25.12 and for `image_editor`.)

- Sprites/textures: `Sprite2D` (or `Visual::Image`) with atlas / animation /
  flip / 9-slice; texture registration already exists in the backends.
  **`Visual::Image { texture, size }` landed for `image_editor`'s
  canvas** — a `Node2D` drawn by `SceneTree::paint` and hit-tested like a rect —
  plus `SceneTree::paint`/`input` handling it. Atlas / animation / flip /
  9-slice and a `Sprite2D` type are still future.
- Primitives: `Path` / `Arc` / `Ellipse` in all three backends + inspector.
  (`Line` landed early in Stage 25.12: `DrawCommand::Line { from, to, paint,
  width }` + `PaintContext::draw_line`, Canvas/wgpu/recording, and `Divider`/
  column separators now use it.)
- Collision: basic AABB / circle queries and `Area` triggers first; rigid bodies
  later (needs rule 5 relaxed — done in this stage).
- Timers / tweens / lightweight signals.
- Assets: image decode / texture loading pipeline.
- Audio: separate crate + backend.

### Phase 7 — native continuous loop + fixed timestep

- `examples/wgpu_demo`: `ControlFlow::Wait` -> `Poll` or `WaitUntil` fixed step.
- Separate logic step from render interpolation (`_physics_process` vs
  `_process`).
- `SubViewport` / offscreen render targets last.

### Phase 8 — observability / tests / docs

- `draw_profile`: canvas-layer and camera-matrix counters / audits.
- Tests: camera golden, layer order, input pick order, world/screen transforms.
- Update `architecture.md`, `backend.md`, `components.md`, `plan.md`,
  `AGENTS.md`.

### Phase 9 — packaging facade (`quill`)

Status: **planned, execute later** (can start once Phase 1 lands; finalized once
`draw_game` exists in Phase 6).

Purpose: keep the fine-grained core crates (they enforce the dependency rules)
but give applications one dependency with opt-in features, so a UI app never
compiles game logic and a game never compiles UI unless it asks.

- Add a facade crate `quill` that only re-exports; optional deps forwarded per
  feature (see the Packaging section).
- UI-only apps depend on `quill` with `ui` + one backend; they never enable
  `game` and therefore never build `draw_game`.
- Fine-grained crates stay separate; the facade does not merge them.

## Dependency order

```
Phase1 -> Phase2 -> Phase3 -> Phase4 -> Phase5 -> Phase6 -> Phase7
                                              \-> Phase8 (continuous)
Phase9 (facade) starts after Phase1, finalizes after Phase6
```

MVP = Phase 1 -> 2 -> 3 -> 4 (world + camera + CanvasLayer UI running).
Phase 5-6 are the second batch.

## Decisions (LOCKED, confirmed)

1. **`Viewport` naming (DONE in Stage 25.2).** `draw_core::Viewport` is only a
   size + DPR helper. Renamed to `draw_core::ViewportSize`; the new scene-level
   render-context node is `draw_scene::Viewport` (root instance the tree root,
   Godot `RootViewport`).
2. **Tree access from `Ui`.** Pass `&mut SceneTree` explicitly to
   `mount` / `layout` / `paint` / `handle_input`. Prefer explicit borrowing over
   `Rc<RefCell<SceneTree>>`.
3. **Migration strategy.** Keep a `Ui::new()` compatibility layer (owns a tree)
   while adding the borrowed API; migrate demos afterwards.
4. **Packaging.** Core crates stay fine-grained (they enforce the boundaries);
   applications use a single facade crate `quill` with opt-in features.
   Game logic lives only in `draw_game`, never in the core, so a UI-only app
   cannot compile it. See the Packaging section. Implementation is deferred to
   Phase 9.

## Packaging — facade crate `quill`

The core is intentionally many small crates: the boundaries are what enforce
AGENTS rule 1 (no backend/DOM in the core) and the dependency direction. Do not
merge them. Instead, add a **facade** so a new project sees one dependency.

Dependency layering:

```
draw_core ──┬─ draw_render ──┬─ draw_scene ── draw_ui ──┬─ draw_components
            │                │                          └─ draw_debug_ui
            ├─ draw_theme ───┘
            ├─ draw_profile
            └─ draw_backend_{canvas,recording,wgpu}

draw_game -> draw_scene (+ optional draw_ui)   [Phase 6]
quill     -> re-exports, feature-gated                    [Phase 9]
```

Minimum for a **UI-only app**: `draw_core`, `draw_render`, `draw_scene`,
`draw_theme`, `draw_ui`, `draw_components` + one backend. It never pulls
`draw_game`, `draw_profile`, `draw_debug_ui` or the benches unless asked.

The `quill` facade feature matrix:

| feature | forwards to | notes |
|---|---|---|
| `ui` | `draw_core`, `draw_render`, `draw_scene`, `draw_theme`, `draw_ui`, `draw_components` | base for any app |
| `game` | `draw_game` | 2D world / sprites / collision; **does not imply `ui`** |
| `wgpu` | `draw_backend_wgpu` | native rendering |
| `canvas` | `draw_backend_canvas` | web rendering |
| `wasm` | `draw_wasm` | browser glue (implies `canvas`) |
| `profile` | `draw_profile` | optional |
| `debug` | `draw_debug_ui` | optional |
| `recording` | `draw_backend_recording` | tests |
| `bench` | `draw_bench`, `draw_bench_suite` | benchmarks |

Applications enable only what they need:

```toml
# desktop UI app
quill = { path = ".../quill", default-features = false, features = ["ui", "wgpu"] }

# web UI app
quill = { path = ".../quill", default-features = false, features = ["ui", "canvas", "wasm"] }

# 2D game (add "ui" only if it wants a HUD)
quill = { path = ".../quill", default-features = false, features = ["game", "wgpu"] }

# headless core (tests / tooling)
quill = { path = ".../quill", default-features = false, features = ["ui", "recording"] }
```

Rules:

- The facade is backend-neutral by default; never force a backend.
- Optional dependencies use `optional = true` + `feature = ["dep:..."]` so
  disabled crates are not compiled at all.
- The facade only re-exports; no logic lives there.
- `game` and `ui` stay independently selectable.

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

Q1/Q3/Q4/Q6 matter for Phases 2-5. Phase 1 landed in Stage 25.1. Q1 is now
resolved from the source (see below); Q3, Q4, Q6 are still open.

### Findings from the Godot tree

- **Q3 (`Control` anchorable rect).** `Control::get_parent_anchorable_rect`
  resolves against the parent canvas item's anchorable rect (`Control` rect,
  else a `CanvasItem`'s), falling back to the viewport visible rect when there
  is no parent canvas item. Anchors therefore resolve against the nearest
  ancestor `Control`, or the viewport. Containers only consider `Control`
  children (`Container::_sort_children` skips non-`Control` children), so a
  mixed subtree is legal but UI containers ignore `Node2D` children — matching
  our `children_of`/control registry.
- **Q4 (GUI pick order).** `Viewport::gui_find_control` iterates `gui.roots`
  **back-to-front** (topmost last); `_gui_find_control_at_pos` recurses children
  back-to-front and returns the topmost visible control whose point test hits
  and whose `mouse_filter != IGNORE`. `_gui_call_input` then bubbles from the hit
  control up `get_parent_item`, consuming pointer events on `MOUSE_FILTER_STOP`
  (except scroll events with `force_pass_scroll_events`) and continuing on
  `PASS`. `Viewport::push_input` order is `_input` -> `_gui_input_event` ->
  `_unhandled_input`, exactly what `Ui::route_input` implements.

- **Q5 (Camera2D transform).** `Camera2D::get_camera_transform` builds a camera
  transform `T = scale(1/zoom)` with origin
  `camera_pos - anchor_offset + offset`, where
  `anchor_offset = anchor_mode == DRAG_CENTER ? screen_size * 0.5 * (1/zoom) : 0`,
  then returns `T.affine_inverse()` as the viewport `canvas_transform`. Negative
  zoom is allowed (mirror) but zero is rejected; limits/drag/rotation/smoothing
  wrap this core. Stage 25.2 ports the core only.

- **Q1 (CanvasLayer vs `canvas_transform`).** A `CanvasLayer` does **not**
  compose with `Viewport.canvas_transform` by default: its effective transform
  is its own (`CanvasLayer::get_final_transform` returns `transform` unless
  `follow_viewport_enabled`, in which case it is
  `viewport.get_canvas_transform() * scale(follow_viewport_scale) * transform`).
  A canvas item's canvas transform is `canvas_layer->get_final_transform()` if it
  has a layer, else `viewport.get_canvas_transform()`; a nested canvas item with
  no layer inherits its parent's (`CanvasItem::get_canvas_transform`). This is
  exactly the Phase 2/3 model: UI under a layer ignores the camera unless asked.
  Note Godot caches the resolved `canvas_layer` pointer on enter-tree, so the
  layer is structural, not re-resolved per frame.

---

## Stage 25.1 — Phase 1 (DONE)

Status: **landed.** `draw_scene` gained a generic per-node extension slot, the
`CanvasLayer` / `Camera2D` node kinds with their dedicated data, and
`SceneTree::canvas_layer_of`. `Node` / `SceneTree` dropped `Clone` (the slot is
`Box<dyn Any>`); `Node: Debug` is manual. No rendering or coordinate change.
See the stage report for the file/test summary.

Original executable plan below (kept for reference).

### Deliverables

1. **Generic node data slot** (the Phase 1 reason-to-exist).
   ```rust
   // draw_scene::Node
   impl Node {
       pub fn set_data<T: 'static>(&mut self, value: T);
       pub fn data<T: 'static>(&self) -> Option<&T>;
       pub fn data_mut<T: 'static>(&mut self) -> Option<&mut T>;
       pub fn has_data<T: 'static>(&self) -> bool;
       pub fn take_data<T: 'static>(&mut self) -> Option<T>;
   }
   ```
   Backing store: `Option<Box<dyn Any>>`. Downcast by `TypeId`.
   `draw_scene` stays generic and backend-neutral; it never names `ControlData`.

2. **Node kinds:** add `NodeKind::CanvasLayer` and `NodeKind::Camera2D`.
   `Camera2D` implies a `CanvasItem` (it is a 2D node with a transform);
   `CanvasLayer` has no `CanvasItem`.

3. **Dedicated engine data** (not the generic slot), owned by `Node`:
   ```rust
   pub struct CanvasLayerData {
       pub layer: i32,
       pub transform: Transform2D,
       pub follow_viewport: bool,
   }
   pub struct Camera2DData {
       pub enabled: bool,
       pub current: bool,
       pub zoom: Vec2,
       pub offset: Vec2,
   }
   ```

4. **Layer resolution:** `SceneTree::canvas_layer_of(id) -> Option<(NodeId, CanvasLayerData)>`
   returns the nearest ancestor `CanvasLayer` (or `None` for the default
   canvas). No transform math yet.

5. **Constructors/exports:** `SceneTree::add_canvas_layer(parent, name)`,
   `SceneTree::add_camera_2d(parent, name)`; re-export the new types from
   `draw_scene::lib`.

### Decision to settle in this stage

`Node` / `SceneTree` currently `derive(Clone)`. `Box<dyn Any>` is not `Clone`.
Nothing in the workspace clones them (checked), so **drop `Clone`** from `Node`
and `SceneTree` and implement `Debug for Node` manually. If a real user appears,
fall back to an `Rc<RefCell<Box<dyn Any>>>` slot.

### Explicitly out of scope

- No `Viewport` node, no camera math, no `SetTransform` change (Phase 2).
- No `CanvasLayer` painting order (Phase 3).
- `SceneTree::paint` keeps painting only world `Visual` (unchanged).

### Tests (native, headless)

- data slot: set / get / get_mut / has / take; wrong-type downcast returns
  `None`; two nodes keep independent data.
- `add_canvas_layer` / `add_camera_2d` create the right `NodeKind`; camera has a
  `CanvasItem`, layer does not.
- `canvas_layer_of` resolves the nearest ancestor; returns `None` without one;
  survives `reparent` out of a layer.
- existing `draw_scene` transform/paint tests stay green (no behavior change).

### Gate

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo bench --workspace --no-run
```

Then emit the Stage 25.1 report and stop for approval before Phase 2.
