# Plan

Working roadmap for the design system (`draw_theme` + `draw_kit`), the drawing
primitives, and the demo. Keep this short and move finished items to the
"Done" section rather than deleting them.

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

## Components (`draw_kit`)

| Component | Status | Notes |
|---|---|---|
| `Text`, `Card`, `Divider`, `Badge`, `Button`, `CodeBlock`, `Terminal`, `EmptyState` | done | |
| `Checkbox`, `Switch` | done | shared `Rc<Cell<_>>` state |
| `Radio` / `RadioGroup` | next | same interaction layer as `Checkbox` |
| `Tabs` | next | active indicator, keyboard focus |
| `Input` / `TextArea` | next | placeholder, caret, selection, focus ring; needs core text editing or a Kit-owned editor |
| `Select` / `Dropdown` | next | menu surface + selected state |
| `Tooltip` | next | floating surface + delay |
| `List` / `Table` | next | header row, column alignment, hover, selection |
| `Toolbar` | next | grouped icon buttons + separators |
| `Modal` / `Toast` | next | floating surface + scrim / transient surface |
| `Progress`, `Spinner`, `Skeleton` | later | uses `Arc`/rounded primitives |
| `ScrollView` | later | needs a clip + offset model |

## Theme

- `Theme` currently resolves at mount time; static surfaces/labels keep their
  colors. Support a **runtime light/dark toggle** by resolving colors at paint
  time (or remounting). `dynamic_surface` already resolves per frame.
- Font weights are not modeled (no weight axis yet) — add `FontWeight` tokens
  when the backends can render them.

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
- Button cursor feedback: `Ui::hovered_is_button` / `Kit::hovered` feed
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
  (`CornerRadii`); `draw_kit` surfaces use them instead of composing circles +
  rects. The demo's list items use square left / rounded right corners with a
  full-height accent bar.
- Centered button/badge text via centered flex labels (measurer-driven, so it
  stays centered after a host injects a real font).
