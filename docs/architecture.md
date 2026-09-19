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

`draw_scene` depends on `draw_render` on purpose: `draw_render` is the
backend-neutral IR (no backend/browser deps) and the Paint step
(Scene -> DrawList) lives in the scene. This does not weaken backend
replaceability.

## Coordinates

Logical pixels are the core unit. Browser device pixel ratio (DPR) is handled
only at the backend/WASM edge and never enters core business logic.

Finalized conventions (Stage 1): origin top-left, `+X` right, `+Y` down,
rotations in radians (positive from `+X` toward `+Y`), rectangles axis-aligned
with half-open membership `[min, max)`. `ViewportSize` stores logical size
only; `ViewportSize::device_size(scale)` derives device pixels without storing
DPR. (The scene-level `draw_scene::Viewport` is a separate render context:
logical size plus the world -> screen `canvas_transform`.)

## Implementation stages

- Stage 0 — workspace skeleton
- Stage 1 — core types / math (`Vec2`, `Rect`, `Transform2D`, `Color`, `NodeId`) [done]
- Stage 2 — `SceneTree` / `Node` / `CanvasItem` / `Node2D` [done]
- Stage 3 — `DrawList` / render IR [done]
- Stage 4 — `RecordingBackend` / headless pipeline [done]
- Stage 5 — Canvas 2D backend + WASM [done]
- Stage 6 — `Control` / layout / input [done]
- Stage 7 — reusable component demo [done]
- Stage 8 — second backend validation (`draw_backend_recording`) [done]. A
  native macOS Core Graphics backend + `macos_demo` was implemented and removed
  by request (not worth the added complexity); do not reintroduce it without an
  explicit ask.
- Stage 9 — `wgpu` backend (`draw_backend_wgpu`, offscreen + pixel readback) [done]
- Stage 10 — performance inspection (`draw_profile`) + debug overlay
  (`draw_debug_ui`) [done]
- Stage 11 — benchmarking (`draw_bench` harness + `draw_bench_suite`) [done]
- Stage 12 — layout engine v2 [done]: intrinsic sizing (`ContentSize`),
  flex (grow/shrink/basis/justify/align/wrap), grid (`Track`, placement,
  spans), and deterministic text wrapping. `VBox`/`HBox` are now thin
  aliases over a column/row `FlexStyle`; `Flex`/`Grid` are components.
- Stage 13 — layout v2 polish [done]: flex `align-content` and separate
  cross-axis gap; grid `align-items`/`justify-items`/`align-content`,
  span-aware auto tracks; per-control `LayoutStyle::order`.
- Stage 14 — text measurement [done]: pluggable `TextMeasurer`
  (`ApproxTextMeasurer`, `FixedWidthTextMeasurer`), `TextOptions`
  (`wrap`/`max_lines`/`ellipsis`). A host injects metrics via
  `draw_ui::set_text_measurer`; layout stays deterministic without font shaping.
- Stage 15 — incremental layout [done]: `Ui` caches the resolved viewport and
  skips measure/arrange unless a dirty flag is set (structure/property/text
  changes, `tree_mut`, measurer swap, or a different viewport).
- Stage 16 — layout/text polish [done]: `TextMeasurer::ascent` for honest
  baselines; a paint-side per-control text-layout cache (invalidated when
  `TextOptions`/measurer/text/width change); per-pass memoization of
  `measure_node`; optional button text wrapping; a visible "missing glyph" box
  in the wgpu font atlas.
- Stage 17 — partial relayout [done]: dirty marks propagate from a changed node
  to its ancestors; a clean subtree whose resolved rect is unchanged is skipped
  entirely, so an isolated text change only re-arranges its own branch. Child
  ordering is cached per container (`order_cache`). `Ui::last_arranged_nodes()`
  reports the work done, and `Ui::invalidate_layout()` forces a full pass.
- Stage 18 — shared demo app [done]: `examples/demo_app` owns the backend-neutral
  `DemoApp` (scene + UI + update/layout/paint/event); `examples/wgpu_demo` and the
  WASM demos only add host glue and (for wgpu) a matching `TextMeasurer`. Its
  native tests verify layout and the full pipeline through
  `draw_backend_recording`.
- Stage 19 — real font stack in the wgpu backend [done]: a font is located from
  `QUILL_FONT` or a per-OS candidate list, parsed with `ab_glyph`, and rasterized
  on demand into a dynamic, shelf-packed atlas (uploaded after each `submit`).
  `FontConfig` selects `FontMode::System` (device-pixel rasterization for crisp
  HiDPI, while metrics stay logical) or `FontMode::Pixel` (built-in bitmap), and
  `WgpuBackend::set_font_config` switches at runtime. Advance/line/ascent metrics
  are exposed as `FontMetrics` so hosts can build a matching
  `draw_ui::TextMeasurer`. The core stays text-free.
- Stage 21 — complex-script shaping [done]: the wgpu backend shapes each line
  with `rustybuzz` (kerning, ligatures, contextual forms) and `unicode-bidi`
  (visual run ordering), rasterizes by glyph id, and aligns runs by the shaped
  advance. `TextMeasurer::measure_run` is the backend-neutral hook so layout
  measures with the same shaping; the Canvas/WASM `measureText` measurer also
  measures whole runs. The core stays text-free.
- Stage 22 — overlay layer [done]: `draw_components::Overlays` owns its own `Ui`
  and provides `confirm` / `popover` / `tips` / `message` builders on top of a
  pure placement module (edge flipping + margin clamp). It handles scrims, modal
  input capture, Esc/click-outside dismissal, auto-dismiss timers and callbacks;
  hosts call `layout`, `paint` and `handle_input` around their own pipeline.
- Stage 23 — per-node decorations [done]: `draw_ui::{NodeDecor, InteractState}`
  plus `Ui::add_decor` / `Ui::state_for` let components attach themed chrome to
  their root node. `Ui::paint` runs decorators around the widget content in one
  pass and `Ui::set_on_click` accepts any control, dispatching to the nearest
  ancestor. The `Kit` runtime is gone: `draw_components` components attach
  decorators, and hosts run a single paint / input pass.
- Stage 24 — declarative views [done, superseded by Stage 25]:
  `draw_ui::{View, BuildContext, ViewExt, Column, Row}` + `Ui::mount`. A view
  tree composes with `.child(..)` and chainable modifiers that post-process the
  built node. Stage 25 replaced this layer with component-native `.child()`.
- Stage 25 — unified scene + component API [in progress]: one `SceneTree` owns
  world and UI. `draw_scene::{Viewport, Camera2D, CanvasLayer}` drive the world
  and layer UI in viewport coordinates. Components are values built with
  `SceneTree::add_child`; every `draw_components::Component` carries a `Spec` and
  supports `.child()`/`.background()`/`.grow()` natively (no `View`/`ViewExt`
  layer). The theme is a `Copy` value passed to constructors — it is no longer
  stored on the tree; the text measurer still lives on the root.
  `draw_ui` is layout + paint free functions plus per-node runtime
  (`ControlData`/`Widget`/decorators/GUI state/layout cache).

## Debugging & performance inspection (Stage 10)

The pipeline stays backend-neutral, and so does observing it. `draw_profile`
never measures time or touches a backend; hosts sample `Instant` per phase and
feed the numbers in:

```text
frame_start -> update -> layout -> paint -> render -> frame_done
                |          |         |         |
                +---------- StageTimes ----+  FrameCounters
                                            |
                     Profiler.record(FrameStats) -> FrameSummary
                                            |
                     inspect(&DrawList, &FrameStats) -> InspectionReport
                                            |
                           draw_debug_ui::DebugOverlay (draw_ui panel)
```

- `Profiler` keeps a bounded frame history and derives averages/min/max/FPS.
- `inspect` produces severity-ranked `Finding`s (correctness, degenerate
  geometry, budgets) aggregated by `FindingCode`.
- `DebugOverlay` draws **component debug bounds**: a yellow border + `Name #id`
  on every visible control, via `draw_ui::Ui::paint_debug`.
- `PerformanceOverlay` renders the summary + findings as an ordinary `draw_ui`
  panel; it is painted after the application UI and does not touch app layout or
  input.

See `docs/debug.md`.

## Benchmarking (Stage 11)

The profiler observes a frame; a benchmark pins a path to a number and guards it
against regression. The harness is dependency-free and lives outside the
pipeline:

```text
draw_bench_suite          ->  draw_bench
deterministic fixtures        BenchRunner -> Stats
drive one stage               Baseline   -> Verdict
```

- `draw_bench` measures and compares only; it never builds a scene or touches a
  backend.
- `draw_bench_suite` builds fixtures and drives scene/ui/render; it contains no
  timing code.
- `draw_backend_wgpu` adds a GPU benchmark for the offscreen render + readback
  path.

`draw_bench` and `draw_bench_suite` sit beside the pipeline (like the demos):
they depend on the core crates but no core crate depends on them. See
`docs/benchmarking.md`.

## Backend replaceability

Same `Scene` + `UI` + `DrawList` must run on any backend without changing
Scene/UI code. Backend-specific code lives only in `draw_backend_*`,
`draw_wasm`, and the examples.

Validated by three independent renderers consuming the same IR:

- `draw_backend_canvas` (HTML Canvas 2D, WASM) — `examples/web_demo`.
- `draw_backend_recording` (headless recording backend) — `tests/pipeline.rs`.
- `draw_backend_wgpu` (native `wgpu`, offscreen target + pixel readback, and
  window-surface presentation) — `crates/draw_backend_wgpu/tests/render.rs`,
  `examples/wgpu_demo`.

Reused unchanged by both: `draw_core`, `draw_scene`, `draw_ui`, and the
`DrawList` / `RenderBackend` contract in `draw_render`.
Backend-specific: command-to-API mapping, resource registration, and the
platform loop/window (`draw_wasm`, the examples).

The native `wgpu_demo` and the WASM `web_demo` additionally share
`examples/demo_app`: the same backend-neutral `DemoApp` drives both, and only
the host glue (window loop / WASM `App` impl) and the injected `TextMeasurer`
differ.
