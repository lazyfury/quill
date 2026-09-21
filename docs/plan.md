# Plan

Working roadmap for the design system (`draw_theme` + `draw_components`), the drawing
primitives, and the demo. Keep this short and move finished items to the
"Done" section rather than deleting them.

> **Stage 25+ is a planned architecture migration** to a Godot-style unified
> scene (single `SceneTree` for world + UI, `Viewport`/`Camera2D`, `CanvasLayer`
> UI in viewport coordinates). That change is specified separately in
> [`docs/godot-migration.md`](godot-migration.md); this file keeps the
> day-to-day item lists below. Where they conflict for Stages 25+,
> `godot-migration.md` wins.

## Drawing primitives

The render IR (`draw_render::DrawCommand`) is the backend-neutral surface. Keep
it monochrome/solid-paint only until a real need appears.

| Primitive | Status | Notes |
|---|---|---|
| `FillRect` / `StrokeRect` | done | axis-aligned |
| `FillCircle` / `StrokeCircle` | done | tessellated fan / ring |
| `FillRoundedRect` / `StrokeRoundedRect` | done | radius clamped to half the smaller side |
| `Line` | done | `from`/`to`/`width`; butt caps; thin quad on wgpu |
| `Arc` / `Ellipse` | planned | spinners, progress rings, gauges |
| `Path` (polyline/polygon) | partial | `draw_svg` strokes SVG paths with `Line` + `FillCircle`; a native `Path` IR primitive is still planned |
| Rounded `ClipRect` | planned | rounded image masks / cards |
| Gradients / patterns | later | `Paint` grows variants without changing command shapes |

Every new primitive must be implemented in **all** backends
(`draw_backend_canvas`, `draw_backend_wgpu`, `draw_backend_recording`) and
audited by `draw_profile`'s inspector, or it is not "done".

## Components (`draw_components`)

| Component | Status | Notes |
|---|---|---|
| `Text`, `Card`, `Divider`, `Badge`, `Button`, `CodeBlock`, `Terminal`, `EmptyState` | done | |
| `Checkbox`, `Switch` | done | shared `Rc<Cell<_>>` state |
| `Radio` / `RadioGroup` | next | same interaction layer as `Checkbox` |
| `Tabs` | next | active indicator, keyboard focus |
| `Input` / `TextArea` | next | placeholder, caret, selection, focus ring; needs core text editing or a component-owned editor |
| `Select` / `Dropdown` | next | menu surface + selected state |
| `Tooltip` | done | `Overlays::tips`, anchored and hover-tracked |
| `List` | done | virtualized: mounts the viewport's rows (+1 buffer) and recycles them; rows come from a `RowSource`; wheel + click + selection; `docs/components.md` |
| `Table` | next | header row, column alignment over the `List` pool (sorting, multi-column, row actions) |
| `Toolbar` | next | grouped icon buttons + separators |
| `Modal` / `Toast` | done | `Overlays::confirm` (scrim) / `Overlays::message` (transient) |
| `Popover` | done | `Overlays::popover`, anchored with edge flipping |
| `Menu` / `MenuItem` | done | floating surface + rows (`shortcut`/`tone`/`disabled`); `Overlays::menu` anchors `BelowStart`; used by `examples/image_editor` |
| `Progress`, `Spinner`, `Skeleton` | later | uses `Arc`/rounded primitives |
| `ScrollView` | next | the clip + offset model now exists (`ControlData.clip`, `set_on_scroll`); wrap it in a component with a draggable scrollbar |

## Theme

- `Theme` is a plain `Copy` value passed to component constructors; nothing
  reads it from the tree. Support a **runtime light/dark toggle** by rebuilding
  the tree with a different `Theme` (or resolving colors per frame in the
  component's `prepare`/decorator). `dynamic_surface_decor` already resolves per
  frame.
- `Theme` also carries a `Density` (spacing scale + control metrics): `compact()`
  is a token swap, and the component library reads `theme.spacing` /
  `control_height` / `row_height`, so a custom theme (e.g. the image editor's)
  changes density without touching components. `Button` takes a `ControlSize`
  (`mini()`/`regular()`), defaulting to the theme's `default_control`.
- Font weights are not modeled (no weight axis yet) — add `FontWeight` tokens
  when the backends can render them.

## Component layer (`draw_components`)

Components are the public construction API: attach with `SceneTree::add_child`,
compose with `.child(..)`, and mutate nodes with `Component` modifiers
(`grow`, `min_size`, `background`, `foreground`, `on_click`, …). `draw_components`
owns `Component` + `Spec`, the base primitives (`draw_components::base`) and the
themed library. Remaining polish, in priority order:

1. **Reactive text bindings** — add `Text::dynamic(|| …)` (a text source on
   labels) so `update()` stops calling `draw_components::set_text`; the runtime
   re-evaluates the closure at layout/paint.
2. **Explicit rect anchors** — allow a popover/menu to anchor to a raw `Rect` or
   pointer position (context menus), not only a laid-out `NodeId`.
3. **Keys + reconciliation** — if a host rebuilds a subtree per frame, add a
   reconciler that diffs by type/key, reusing the existing dirty tracking /
   partial relayout.
4. **`view!` macro (optional sugar)** — a `draw_macros` proc-macro expanding to
   the builder calls, e.g. `view! { Card(gap = 12.0) { Text("Hi") } }`.
5. **Runtime mutation helpers** — keep `update_control` / `set_text` /
   `set_on_click` for hosts that animate one node; construction stays
   component-only.
6. **Ring-indexed list pool** — `List` re-binds every row in the pool on a scroll
   step (stepping 2.5 rows re-binds all 35: 73 µs/frame measured against 5.8 µs
   for a frame that does not move). Keying slots by `index % pool_size` would
   re-bind only the rows entering and leaving, which is worth ~12x on the scroll
   path. Deferred: the current cost is 0.4% of a 60 Hz budget.
7. **Wheel pump in the other two hosts** — `examples/file_browser` translates
   winit's `MouseScrollDelta` into `InputEvent::Wheel` (`host::wheel_pixels`, a
   pure function with tests); `wgpu_demo` and the Canvas runner (DOM `wheel`)
   still need the same few lines, and until they have them a `List` inside them
   simply does not scroll.

## UI runtime — `Ui` boundary & lifecycle (folded into Stage 25)

> This section is now part of the Godot-style migration; see
> [`docs/godot-migration.md`](godot-migration.md) Phase 4 (single tree) and
> Phase 5 (lifecycle/input). The items below are the constraints that phase
> must satisfy.

`Ui` is necessary as the retained UI document + layout/paint/input runtime, but
it is currently a god object and overlaps `SceneTree` on "who owns a control".
Godot puts `Control` data on the node; here `draw_scene` stays a generic draw
graph and UI data lives in `Ui`'s `NodeId`-keyed maps. Priority order:

**Resolved (Stage 25).** The `Ui` object was removed entirely; `draw_ui` is now
free functions over the tree, and every per-control runtime value
(`ControlData`/`Widget`/decorators/callback) lives on the node's extension slot.
The layout cache, GUI interaction state and text measurer live on the root; the
**theme is not stored on the tree**. See `docs/godot-migration.md` Phase 4.

## Core hardening (design-review follow-ups)

Findings from a design review of `draw_core` / `draw_render` / `draw_scene` /
`draw_ui` / `draw_components`. All fixes keep the core backend-neutral; land them in
priority order and add native tests.

| # | Item | Severity | Where |
|---|---|---|---|
| 1 | Extension slot is **single-type**: `Node::set_data` replaces the one `Box<dyn Any>`, so storing app data on a `Control` node silently destroys its `Control` runtime, and a node cannot hold both. Make the slot type-keyed (`HashMap<TypeId, Box<dyn Any>>`) or give `Control` a dedicated field. | high | `draw_scene/src/node.rs:369`, `tree.rs:176` |
| 2 | `SceneTree::paint` includes **every** canvas item with a `Visual`, not just `Node2D`; a `Control` with a `Visual` would be painted twice (scene + `draw_ui`). Filter by node kind / ownership. | medium | `draw_scene/src/paint.rs:66` |
| 3 | **Resource lifecycle is not in the IR contract**: `RenderBackend` has no texture registration; each backend registers privately (e.g. Canvas `register_image`). Document it as a backend extension point, and consider a minimal `register_texture` contract. | medium | `draw_render/src/backend.rs`, `texture.rs` |
| 4 | `DrawCommand::DrawText` owns a `String` (one allocation per text command per frame). Revisit (`Rc<str>`/interned text) only if it shows in the benchmarks. | low | `draw_render/src/command.rs` |
| 5 | Stale docs: `draw_scene` crate doc claims it must not depend on `draw_render` (it does, by design); `Overlays` module doc/example still says it owns a `Ui` and calls `app.ui.*`. | low (docs) | `draw_scene/src/lib.rs:4`, `draw_components/src/overlay/mod.rs:1` |
| 6 | Naming: cross-link `draw_core::ViewportSize` vs `draw_scene::Viewport` docs; clarify that the internal zero-sized `draw_ui` `Ui` namespace is not a public object. | low | `draw_core`, `draw_ui/src/ui/mod.rs` |

## Demo (`examples/demo_app`)

- Light/dark toggle in the sidebar.
- Scrollable note list (depends on `ScrollView`).
- Keyboard navigation (arrow keys move list selection; `⌘K` command palette).
- Command palette overlay using the `List`/`Input` components.

## Demo (`examples/image_editor`)

A Photoshop-style editor built on `draw_ui` / `draw_components` (own workspace,
see its README). Phases 1–8 landed. The next three reuse the overlay layer
(`draw_components::Overlays`) and the worker-thread / `EventLoopProxy` pattern
from `examples/file_browser`:

- **Phase 9 — menu bar (done).** The menu bar opens real drop-downs: a title
  click records a request that the view drains into `Overlays::menu`, whose
  content is a `Menu` of `MenuItem`s (label + right-aligned shortcut, disabled
  when the action is unavailable). Undo/redo, import/export, zoom and
  clear-selection are wired; the rest are labeled placeholders. `Menu`/
  `MenuItem` and `Overlays::menu` live in `draw_components`, so Phase 11 reuses
  them.
- **Phase 10 — file browser.** Replace the inline path field in the file panel
  with a picker overlay: a `List` of directory entries filled by a scan on a
  worker thread (`EventLoopProxy`, copied from `examples/file_browser`),
  double-click / Enter to navigate, a `*.png` filter, and picking a file drives
  import (and sets the export path). First consumer of `List` + a worker thread
  inside a second host.
- **Phase 11 — layer context menu.** Right-click a layer row opens a context
  menu at the pointer (rename / duplicate / delete / hide / move / merge down /
  opacity). Two additive prerequisites:
  1. `Overlays` anchored to a **raw rect or pointer position**, not only a
     laid-out `NodeId` (Component-layer item 2 above).
  2. Right-click routing: `PointerButton::Right` exists, but
     `draw_ui::handle_input` only reacts to the left button. Add an additive
     context callback (`Control::context_callback` +
     `Component::on_context_menu`) so any control can own a context menu, rather
     than the host hit-testing rows itself.

Cross-cutting: Phases 9 and 11 both need "a popover full of commands", so the
`Menu` component should land first and be reused by both.

## Invariants

- The core (`draw_core`, `draw_scene`, `draw_render`, `draw_ui`) stays
  backend-neutral; new layers only use public APIs.
- API -> test -> implementation -> integration.
- No screenshot/screen-recording verification; assert `DrawList` commands,
  layout rects, callbacks and backend pixel buffers.
- Run the per-stage gate before merging: `cargo fmt --all -- --check`,
  `cargo check --workspace`, `cargo test --workspace`,
  `cargo bench --workspace --no-run`.

## Done

- `draw_svg` (`crates/draw_svg`): backend-neutral SVG vector rendering with **no
external dependency**. It parses a small SVG subset (the Lucide grammar:
`path`/`rect`/`circle`/`ellipse`/`line`/`polyline`/`polygon`, full path data
incl. arcs, `stroke`/`stroke-width`/`stroke-linecap`/`stroke-linejoin`/`viewBox`)
into flattened polylines and strokes them with the existing `Line` +
`FillCircle` IR, so it works with any backend. `IconPack` indexes a directory of
`.svg` files by name. Validated against the whole Lucide pack: 2 112 icons parse
into 8 605 shapes / ~224 K commands with none empty
(`DRAW_SVG_ICON_DIR=… cargo test -p draw_svg -- --ignored every_icon`). See
`docs/svg.md`. Fills are not rendered yet (stroke-only), which is exactly the
Lucide case.
- `List` + the core increments it needed: `ControlData.clip` (the first and only
  source of `DrawCommand::ClipRect`, resolved in the layout pass, emitted as one
  save/clip/restore per clipped region, respected by hit testing),
  `InputEvent::Wheel` routing to the nearest ancestor with a scroll callback
  (`draw_components::set_on_scroll` / `Component::on_scroll`), and
  `draw_components::List` + `ListState` — a virtualized list that mounts
  `ceil(viewport/row) + 1` rows and recycles them, so its frame cost is flat in
  the row count. Measured: 107 controls and 72 commands per scrolling frame at
  1 K, 10 K and 100 K rows (73 µs) against a naive mounted-everything list's
  3 000 / 30 000 / 300 000 controls (1.75 ms → 261 ms). See
  `docs/components.md` and `docs/benchmarking.md`.
- `examples/file_browser`: the first real consumer of `List` — a directory
  browser with the scan on a worker thread, keyboard + wheel navigation, and a
  headless `--selfcheck` that asserts the frame does not grow when the listing
  goes from 5 000 to 200 000 rows (85 controls, 64 commands, 19 pooled rows at
  900x620). It is also the first host to translate a platform wheel into
  `InputEvent::Wheel` (`host::wheel_pixels`). Its right pane is a **resizable
  split** (`ResizeHandle::vertical` driving the main pane's basis) holding a
  second `List`: the selected file's first 64 KiB, shown either as a hex dump or
  as text (`PreviewMode::{Binary,Text}`, toggled with `T` or by clicking the pane's
  tab) — the bytes are read once and the mode only changes how rows are computed,
  so switching is free. The two modes are two `List`s (columns are fixed at build
  time) chosen by `SceneTree::set_visible`; a hidden list's container has zero
  height, so `ListState::sync` returns early and it owns no pool at all. So 4 096
  rows of data cost the same ~30 mounted rows as 64 do. Two notes for whoever
  copies the shape: a handle's `min`/`max` are build-time constants and cannot see
  the viewport, so a resizing host must re-clamp on `layout`; and every
  virtualized list in a view needs its own `ListState::sync` in the same frame
  step.
- **Text mode cannot show indentation**: `draw_ui`'s wrapping is word-based and
  collapses leading whitespace (`draw_ui::layout::text::wrap_hard_line`), and
  `List` builds its row cells with `max_lines(1)` + ellipsis, so a line's leading
  spaces never reach the screen. Fixing it means a per-column `TextOptions.wrap`
  on `ListColumn` (or a non-wrapping cell), which is an API change nobody has
  asked for yet; the row content itself is correct.
- `draw_core::Key` has no `PageUp` / `PageDown`, so list UIs cannot map a
  page-step key yet (the browser falls back to arrows + `Home` / `End`). Adding
  the two variants is additive and would let `List` offer a page step.

- `Router` view switching: `draw_components::Router` shows exactly one child
  view at a time by toggling scene visibility from a shared route cell. Layout,
  paint and hit-testing now skip controls hidden at runtime (flex lines drop
  them and hidden nodes get no rect). `demo_app`'s detail pane is a router with
  a note view and a settings view.

- Split the old `draw_app` crate: **input routing** moved into `draw_ui`
  (`hit_test` / `handle_input` / `route_input` / `hovered` / `hovered_cursor` /
  `focused` / `is_interactive`), next to the `ControlData` it operates on, and the
  **construction layer** folded into `draw_components::base`
  (`Component`/`Spec`/`Flex`/`Panel`/`Label`/`base::Button`/`Grid`,
  `impl_scene_child!`, mutation helpers). Result: `draw_ui` (layout/paint/input
  engine) + `draw_components` (base + themed widgets) — no separate
  `draw_widgets` crate.
- Removed the unused `draw_app::App` runtime: it duplicated the host's frame
  loop and its `render` never painted world (`Node2D`) visuals. Frame submission
  now lives with the host (`draw_ui::layout`/`paint` + `draw_scene` paint ->
  `RenderBackend`), which is what every demo already did. See the review item
  "Core hardening".

- `Line` primitive: `DrawCommand::Line { from, to, paint, width }` +
  `PaintContext::draw_line`, implemented in Canvas (`moveTo`/`lineTo`/`stroke`),
  wgpu (thin quad) and recording. `Divider`/column separators now draw a real
  line instead of a filled rect; the profiler audits line geometry.
- Component-native composition (Stage 25): `SceneTree::add_child` is the single
  attachment point and every `draw_components::Component` supports `.child()` and the
  other modifiers directly. The `View`/`ViewExt`/`BuildContext`/`Modify` layer
  was deleted; `draw_components` no longer exposes `add_*`/`mount` free functions.
  `demo_app`, `Overlays` popover content and the debug overlay use the new API.
- Decorator-based chrome, no `Kit` (Stage 23): components attach
  `draw_ui::NodeDecor` (surface / foreground) and register clicks with
  `draw_components::set_on_click`. A single `draw_ui::paint` / `draw_ui::handle_input`
  runs everything. The theme is a value passed to constructors.
- Overlay layer (`draw_components::Overlays`): a generic floating layer with `confirm`,
  `popover`, `tips` and `message` built on a pure placement module (flip + clamp),
  scrims, modal capture, Esc/click-outside dismissal and auto-dismiss timers.
  `draw_components::Button` gained `Destructive`. Wired into `demo_app` (Delete →
  confirm → toast).
- Fixed flex cross-axis `Stretch` overflowing a definite container: items now
  fill the container's inner cross size instead of growing to their content's
  preferred width, so a fixed-width column's items no longer push past its edge.
  Covered by `draw_ui::ui::layout::stretch_does_not_grow_a_definite_cross_axis`
  and `demo_app::note_rows_fit_with_a_wide_measurer`.
- Stage 21 (complex-script shaping): the wgpu backend shapes each line with
  `rustybuzz` (kerning, ligatures, contextual forms) and `unicode-bidi` (visual
  run ordering), rasterizes by glyph id, and aligns by shaped advances.
  `TextMeasurer::measure_run` is the backend-neutral hook so layout measures with
  the same shaping; Canvas/WASM `measureText` measures whole runs too.
- Button cursor feedback: `Ui::hovered_is_button` / `Ui::is_interactive` feed
  `App::pointer_cursor`, so the Canvas runner sets a `pointer` CSS cursor while
  the pointer is over a clickable control (and `default` otherwise);
  `demo_app::DemoApp::pointer_over_clickable` combines both, and `wgpu_demo`
  maps it to `CursorIcon::Pointer`.
- Canvas/WASM text is vertically centered: `draw_wasm::CanvasTextMeasurer`
  measures with the same `measureText` font the Canvas backend draws with
  (shared `draw_backend_canvas::font_spec`), so layout baselines use the real
  ascent instead of the default `0.8em` guess. The runner hands the context to
  the app via `App::attach_context`, and the WASM demos inject the measurer.
- Exact-fit text no longer wraps from float rounding (the wrap loop sums
  advances in a different order than the natural width); this fixes single-line
  UI text like the "All Notes" list header wrapping at its space.
- Rounded rectangles are first-class `DrawCommand`s with **per-corner radii**
  (`CornerRadii`); `draw_components` surfaces use them instead of composing circles +
  rects. The demo's list items use square left / rounded right corners with a
  full-height accent bar.
- Centered button/badge text via centered flex labels (measurer-driven, so it
  stays centered after a host injects a real font).
