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
| `Line` | done | `from`/`to`/`width`; square caps; thin quad on wgpu |
| `Arc` / `Ellipse` | planned | spinners, progress rings, gauges |
| `Path` (polyline/polygon) | planned | charts, icons, freeform shapes |
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
| `List` / `Table` | next | header row, column alignment, hover, selection |
| `Toolbar` | next | grouped icon buttons + separators |
| `Modal` / `Toast` | done | `Overlays::confirm` (scrim) / `Overlays::message` (transient) |
| `Popover` | done | `Overlays::popover`, anchored with edge flipping |
| `Progress`, `Spinner`, `Skeleton` | later | uses `Arc`/rounded primitives |
| `ScrollView` | later | needs a clip + offset model |

## Theme

- `Theme` is a plain `Copy` value passed to component constructors; nothing
  reads it from the tree. Support a **runtime light/dark toggle** by rebuilding
  the tree with a different `Theme` (or resolving colors per frame in the
  component's `prepare`/decorator). `dynamic_surface_decor` already resolves per
  frame.
- Font weights are not modeled (no weight axis yet) — add `FontWeight` tokens
  when the backends can render them.

## Component layer (`draw_app` / `draw_components`)

Components are the public construction API: attach with `SceneTree::add_child`,
compose with `.child(..)`, and mutate nodes with `Component` modifiers
(`grow`, `min_size`, `background`, `foreground`, `on_click`, …). `draw_app` owns
`Component` + `Spec` and the layout primitives; `draw_components` owns the
themed library. Remaining polish, in priority order:

1. **Reactive text bindings** — add `Text::dynamic(|| …)` (a text source on
   labels) so `update()` stops calling `draw_app::set_text`; the runtime
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

## Demo (`demos/demo_app`)

- Light/dark toggle in the sidebar.
- Scrollable note list (depends on `ScrollView`).
- Keyboard navigation (arrow keys move list selection; `⌘K` command palette).
- Command palette overlay using the `List`/`Input` components.

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

- `Line` primitive: `DrawCommand::Line { from, to, paint, width }` +
  `PaintContext::draw_line`, implemented in Canvas (`moveTo`/`lineTo`/`stroke`),
  wgpu (thin quad) and recording. `Divider`/column separators now draw a real
  line instead of a filled rect; the profiler audits line geometry.
- Component-native composition (Stage 25): `SceneTree::add_child` is the single
  attachment point and every `draw_app::Component` supports `.child()` and the
  other modifiers directly. The `View`/`ViewExt`/`BuildContext`/`Modify` layer
  was deleted; `draw_app` no longer exposes `add_*`/`mount` free functions.
  `demo_app`, `Overlays` popover content and the debug overlay use the new API.
- Decorator-based chrome, no `Kit` (Stage 23): components attach
  `draw_ui::NodeDecor` (surface / foreground) and register clicks with
  `draw_app::set_on_click`. A single `draw_ui::paint` / `draw_app::handle_input`
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
