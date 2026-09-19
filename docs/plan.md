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
| `Line` | planned | needed for dividers, diagrams, chart axes |
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

- `Theme` currently resolves at mount time; static surfaces/labels keep their
  colors. Support a **runtime light/dark toggle** by resolving colors at paint
  time (or remounting). `dynamic_surface` already resolves per frame.
- Font weights are not modeled (no weight axis yet) — add `FontWeight` tokens
  when the backends can render them.

## View layer (`draw_ui`)

Declarative views are the public construction API (`View` / `BuildContext` /
`ViewExt` / `Ui::mount`); the retained `Ui` + `Widget` are the runtime. Remaining
polish, in priority order:

1. **Reactive text bindings** — add `Text::dynamic(|| …)` (a text source on
   labels) so `update()` stops calling `ui.set_text`; the runtime re-evaluates the
   closure at layout/paint. This removes the last `set_*` from app code.
2. **Container-aware modifiers** — `Modify<V>` only post-processes its node, so
   `.child(..)` must precede any `ViewExt` modifier. Make container modifiers
   (`grow`/`padding`/`gap`) forward `child`/`children` so the two interleave
   freely.
3. **Explicit rect anchors** — allow a popover/menu to anchor to a raw `Rect` or
   pointer position (context menus), not only a laid-out `NodeId`.
4. **Keys + reconciliation** — if a host rebuilds the view tree per frame, add
   `ViewExt::key` and a reconciler that diffs by type/key into `Ui`, reusing the
   existing dirty tracking / partial relayout.
5. **`view!` macro (optional sugar)** — a `draw_macros` proc-macro expanding to
   the builder calls, e.g. `view! { Card(gap = 12.0) { Text("Hi") } }`.
6. **Migrate remaining imperative hosts** — `component_demo` and any lingering
   `ui.add` + `ui.set_*` construction; keep `Ui::set_*` runtime-internal only.

## UI runtime — `Ui` boundary & lifecycle (folded into Stage 25)

> This section is now part of the Godot-style migration; see
> [`docs/godot-migration.md`](godot-migration.md) Phase 4 (single tree) and
> Phase 5 (lifecycle/input). The items below are the constraints that phase
> must satisfy.

`Ui` is necessary as the retained UI document + layout/paint/input runtime, but
it is currently a god object and overlaps `SceneTree` on "who owns a control".
Godot puts `Control` data on the node; here `draw_scene` stays a generic draw
graph and UI data lives in `Ui`'s `NodeId`-keyed maps. Priority order:

1. **Boundary** — public `Ui` shrinks to the runtime surface (`mount`, `layout`,
   `paint`, `handle_input`, `theme`/`set_theme`, `set_text_measurer` + read-only
   queries). Move `set_*` / `insert` / `add_decor` behind `BuildContext`
   (`pub(crate)`), so construction is exclusively `View`. Merge `Component` into
   `View` (or make it `pub(crate)`) — one construction abstraction, not three
   (`Widget` runtime / `Component` mount / `View` build).
2. **Ownership & lifecycle** — document that `SceneTree` is the hierarchy and
   `Ui` is the control table over it, then add `Ui::remove` that synchronously
   drops `controls` / `widgets` / `decorations` / `callbacks` for the subtree.
   Long term: consider moving `Widget`/`ControlData` onto `SceneTree` control
   nodes (Godot-style single source of truth); the cost is `draw_scene` knowing
   about widgets.
3. **Theme** — either accept and record "`Ui` is the UI runtime and owns the
   active theme", or introduce an explicit `Environment`/`Context` inherited
   from the root so the core stays theme-free. Fix the mount-time snapshot so a
   theme swap updates live surfaces.
4. **Naming** — `Ui` -> `UiRoot`/`UiDocument` to disambiguate "the UI" from "the
   runtime instance"; only if it grows further, split `Layout`/`Painter` out and
   leave `Ui` as a facade.

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

- Declarative views (Stage 24): `Ui::mount` + `View`/`ViewExt` compose UI with
  `.child(..)` and chainable modifiers (`grow`, `min_size`, `background`,
  `dynamic_background`, `on_click`, `capture`, …); `Column`/`Row` are the
  standard containers and `Component` is blanket a `View`. `demo_app` and the
  overlay popover content are built as view trees.
- Decorator-based chrome, no `Kit` (Stage 23): `Ui` owns the `Theme`
  (`Ui::theme`/`set_theme`); components implement `draw_ui::Component`, read
  `ui.theme()` and attach `draw_ui::NodeDecor` (surface / foreground) while
  registering clicks with `Ui::set_on_click`. A single `ui.paint` /
  `ui.handle_input` runs everything.
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
