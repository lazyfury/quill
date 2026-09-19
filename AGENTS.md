# AGENTS.md — quill

Concise working agreement. Details live in `docs/` (see the map at the bottom);
this file is the short source of truth for rules and status.

## Goal

Backend-neutral 2D/UI drawing core in Rust. Canvas 2D (WASM) is the first
backend. Godot-inspired: `SceneTree -> Node -> CanvasItem -> Node2D / Control`.

## Pipeline (must hold)

```
Input -> SceneTree -> Update -> Layout -> Paint -> DrawList -> RenderBackend -> Pixels
```

## Hard rules

1. `draw_core`, `draw_scene`, `draw_ui`, `draw_render` MUST NOT depend on
   `web_sys` / `wasm_bindgen` / DOM / `wgpu` / any concrete backend.
2. `DrawCommand` holds only backend-neutral data (no Canvas/WebGL/WGPU objects).
3. Scene/UI must be testable with native `cargo test`, no browser.
4. Resources use handles (`NodeId`, `TextureId`), not backend objects.
5. No ECS, shaders, render graph, particles, editor in MVP. Basic 2D
   collision/physics IS allowed from Stage 25 (see `docs/godot-migration.md`);
   a general rigid-body solver / editor remains out of scope until a later
   explicit ask.
6. Do not merge stages. Each stage ends with a report and waits for user approval.
7. **No screenshot / screen-recording visual testing.** Never use
   `screencapture`, browser screenshots, screen recording, or any OS-level
   capture to verify rendering. Verify programmatically instead: read the
   backend's own pixel buffer, assert `DrawList` command sequences, or read DOM
   state markers. If a claim cannot be verified without a screenshot, say so
   rather than capturing one.
8. **The design-system layers do not extend the core.** `draw_theme` (tokens)
   and `draw_components` (components) may only use the public APIs of `draw_core`,
   `draw_scene`, `draw_render` and `draw_ui`. Keep `draw_ui::Widget` and the
   backend-neutral core frozen unless a change is genuinely required and
   backward compatible; record any such change in `docs/design-system.md`.
   Exact token names/paths matter: use `theme.palette.*` and `theme.surface(level)`
   rather than hard-coding hex values in components. Dark is a token swap, not a
   second code path, and dark values must stay within the documented palette.
   **Migration exception (Stage 25+):** the Godot-style migration
   (`docs/godot-migration.md`) may change `draw_scene` / `draw_ui` incompatible;
   keep the compatibility layer green per phase and update
   `docs/design-system.md` when component-facing APIs move.

## Dependency direction

```
draw_core            (no draw_* deps)
draw_theme   -> draw_core
draw_scene    -> draw_core, draw_render
draw_ui         -> draw_core, draw_scene, draw_render
draw_widgets        -> draw_core, draw_scene, draw_render, draw_ui
draw_components -> draw_core, draw_widgets, draw_scene, draw_ui, draw_render, draw_theme
draw_render   -> draw_core
draw_profile  -> draw_core, draw_render
draw_debug_ui -> draw_core, draw_scene, draw_render, draw_ui, draw_widgets, draw_profile
draw_backend_* -> draw_render, draw_core
draw_wasm     -> draw_render, draw_backend_canvas, draw_core, draw_ui
draw_bench    (std only, no draw_* deps)
draw_bench_suite -> draw_bench, draw_core, draw_render, draw_scene, draw_ui, draw_widgets
demo_app      -> draw_core, draw_render, draw_scene, draw_ui, draw_widgets,
                 draw_theme, draw_components   (no backend)
component_demo -> draw_core, draw_render, draw_scene, draw_ui, draw_widgets, draw_wasm
web_demo      -> draw_core, draw_scene, demo_app, draw_wasm
wgpu_demo     -> draw_core, draw_render, draw_scene, draw_ui, demo_app,
                 draw_backend_wgpu, draw_profile, draw_debug_ui, winit
```

Planned (Stage 25, see `docs/godot-migration.md`):

```
draw_game -> draw_scene (+ optional draw_ui)      # Phase 6
quill     -> feature-gated re-exports of the above # Phase 9 facade
```

The core crates stay fine-grained on purpose; applications use the `quill`
facade with opt-in features (`ui`, `game`, `wgpu`, `canvas`, `wasm`, `profile`,
`debug`, `recording`, `bench`). A UI-only app must not compile `draw_game`.

`draw_scene -> draw_render` is intentional: `draw_render` is the backend-neutral
IR (no backend/browser deps), and the Paint step (Scene -> DrawList) lives in the
scene. This does not weaken backend replaceability.

Browser APIs only in `draw_backend_canvas`, `draw_wasm`, and the WASM demos
(`demos/web_demo`, `demos/component_demo`).
`winit` only in `demos/wgpu_demo`. `wgpu` only in `draw_backend_wgpu` (plus its
tests/bench) and `demos/wgpu_demo`. Font parsing (`ab_glyph`), text shaping
(`rustybuzz`, `unicode-bidi`) and system-font discovery live only in
`draw_backend_wgpu`; the core stays text-free.

## Stages

- [x] Stage 0 — workspace skeleton
- [x] Stage 1 — core types / math
- [x] Stage 2 — SceneTree / Node / CanvasItem
- [x] Stage 3 — DrawList / render IR
- [x] Stage 4 — RecordingBackend / headless tests
- [x] Stage 5 — Canvas2D backend + WASM
- [x] Stage 6 — Control / layout / input
- [x] Stage 7 — reusable component demo
- [x] Stage 8 — second backend validation (`draw_backend_recording`)
- [x] Stage 9 — wgpu backend (`draw_backend_wgpu`, offscreen + pixel readback)
- [x] Stage 10 — performance inspection (`draw_profile`) + debug overlay
      (`draw_debug_ui`)
- [x] Stage 11 — benchmarking (`draw_bench` harness + `draw_bench_suite`)
- [x] Stage 12 — layout engine v2: intrinsic sizing (`ContentSize`),
      flex (grow/shrink/basis/justify/align/wrap), grid (`Track`), and
      deterministic text wrapping (`layout::text`)
- [x] Stage 13 — layout v2 polish: flex `align-content` + cross gap; grid
      `align-items`/`justify-items`/`align-content`, span-aware auto tracks;
      `LayoutStyle::order`
- [x] Stage 14 — pluggable `TextMeasurer` (default `ApproxTextMeasurer`,
      `FixedWidthTextMeasurer`), `TextOptions` (`wrap`/`max_lines`/`ellipsis`)
- [x] Stage 15 — incremental layout: dirty flag + viewport cache
      (`Ui::layout_count`, `Ui::invalidate_layout`, `Ui::set_text_measurer`)
- [x] Stage 16 — layout/text polish: `TextMeasurer::ascent` baselines, paint-side
      text-layout cache, per-pass measure memoization, button text wrapping,
      wgpu missing-glyph box
- [x] Stage 17 — partial relayout: per-node dirty propagation, clean-subtree
      skipping (`Ui::last_arranged_nodes`), cached child ordering (`order_cache`)
- [x] Stage 18 — shared `demos/demo_app` used by `wgpu_demo` and the WASM demos,
      with headless layout/pipeline tests through `draw_backend_recording`
- [x] Stage 19 — real font stack in `draw_backend_wgpu`: `FontConfig` chooses
      `FontMode::System` (system font via `QUILL_FONT` or a per-OS list,
      `ab_glyph`, dynamic atlas, device-pixel rasterization for crisp HiDPI) or
      `FontMode::Pixel` (built-in bitmap); `set_font_config` switches at runtime
      and `FontMetrics` lets `wgpu_demo` inject a matching `TextMeasurer`.
- [x] Stage 20 — design system: `draw_theme` design tokens (light/dark
      palettes, spacing/radius/type/motion scales) + `draw_components` themed component
      library (`Text`, `Card`, `Divider`, `Badge`, `Button`, `CodeBlock`,
      `Terminal`, `EmptyState`, `Checkbox`, `Switch`) built on frozen `draw_ui`
      primitives, plus the shared `demo_app` rewritten as a three-column
      macOS-style notes app (icons/images are monochrome placeholder squares).
- [x] Stage 21 — complex-script shaping: the wgpu backend shapes each line with
      `rustybuzz` (kerning, ligatures, contextual forms) and `unicode-bidi`
      (visual run ordering), rasterizing by glyph id and reusing shaped advances
      for alignment. `TextMeasurer::measure_run` (default: sum of advances) lets
      layout measure with the same shaping; the Canvas/WASM `measureText`
      measurer uses it too. The core stays text-free.
- [x] Stage 22 — overlay layer (`draw_components::Overlays`): a generic floating layer
      (own `Ui`) with `confirm`, `popover`, `tips` and `message` builders,
      edge-aware placement with flipping (`overlay::placement`), scrims, input
      capture/modal blocking, Esc/click-outside dismissal, auto-dismiss timers
      and `on_confirm`/`on_cancel`/`on_close` callbacks. `draw_components::Button`
      gained a `Destructive` variant.
- [x] Stage 23 — per-node decorations, no `Kit`: added
      `draw_ui::{NodeDecor, InteractState}`, `Ui::add_decor`, `Ui::state_for`,
      `Ui::is_interactive`; `Ui::paint` runs `paint_behind` / content /
      `paint_front` per node and `Ui::set_on_click` accepts any control and
      dispatches to the nearest ancestor. `Ui` owns the `Theme`
      (`Ui::theme`/`set_theme`, so `draw_ui -> draw_theme`). The `Kit` runtime is
      gone: `draw_components` components implement `draw_ui::Component`, read
      `ui.theme()` and attach `draw_ui::{surface_decor, dynamic_surface_decor,
      foreground_decor}`. Hosts run a single `ui.paint` + `ui.handle_input`.
      `draw_components` holds only component builders; the surface/tone/decorator
      primitives live in `draw_ui`.
- [x] Stage 24 — declarative views: `draw_ui::{View, BuildContext, ViewExt,
      Column, Row}` and `Ui::mount`. Views compose with `.child(..)`; `ViewExt`
      modifiers (`grow`, `min_size`, `anchors`/`offsets`, `background`,
      `dynamic_background`, `foreground`, `on_click`, `capture`, …) wrap a view
      and post-process its node, so `ui.set_*` never appears in app code. Every
      `Component` is automatically a `View` (blanket impl). `Card` takes
      children; `demo_app` and the overlay popover content are built as view
      trees.
      Recorded in `docs/design-system.md`. **Superseded by Stage 25.10/25.11:** the
      `View`/`ViewExt`/`BuildContext`/`Modify` layer and the `add_*`/`mount`
      helpers were deleted; components compose natively with `.child()`.
- [ ] Stage 25 — Godot-style unified scene (planning approved; Phases 1-5,
      the UI-state migration and the component-native API complete, Phase 6
      next). One `SceneTree` for world + UI, `Viewport`/`Camera2D` driving the
      world, and UI under a `CanvasLayer` in viewport coordinates; every node
      owns its own state and `draw_ui` is a set of free functions over the tree
      (no `Ui` object). Full phase plan, target architecture, decisions and open
      questions: `docs/godot-migration.md`. Phases: 1 `draw_scene` extension
      point + layers (done), 2 `Viewport`/`Camera2D` (done), 3 `CanvasLayer`
      painting (done), 4 unified tree: 4a `Ui` borrows the tree, 4b
      `ControlData` onto the node slot, 4c migrate demos, 4d all control runtime
      + GUI state onto nodes, 4e layout cache onto the root, 4f theme + measurer
      onto the root (theme now a passed-in value again, 25.11), 4g remove
      `Ui`/`UiHost` in favor of free functions (done), 5 unified lifecycle/input
      (done), 6 `draw_game` capabilities, 7 native continuous loop, 8
      observability/tests/docs.
      **Stage 25.10/25.11 (component-native API):** `draw_scene::SceneChild` +
      `SceneTree::add_child`; `draw_widgets::Component` carries a `Spec` and exposes
      modifiers as methods; `draw_components` components take the `Theme` as a
      `Copy` value; the theme is no longer stored on the tree.
      **Stage 25.12 (`Line` primitive):** `DrawCommand::Line { from, to, paint,
      width }` + `PaintContext::draw_line`, implemented in Canvas / wgpu /
      recording; `Divider` and column separators draw a real line.
      **Stage 25.13 (drag + resize):** `GuiState.dragging` / `Control.drag_callback`
      with pointer capture in `draw_ui::handle_input`; `Component::on_drag`
      (`DragPhase::{Start,Move,End}` + delta) / `draw_widgets::set_on_drag`;
      `draw_components::ResizeHandle` (a divider-styled gutter that resizes a
      target pane's flex basis). `draw_core::Cursor` + `ControlData.cursor` +
      `Component::dynamic_cursor` (per-control provider) +
      `draw_ui::hovered_cursor`; hosts map it (winit `CursorIcon`, canvas CSS
      `cursor`). `demo_app`'s sidebar and list gutters are both draggable.

## Per-stage gate (must run)

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo bench --workspace --no-run
```

Then emit the report and stop for approval.

## Recurring decisions (do not undo)

- Stage 8: the second backend is `draw_backend_recording`. A native macOS Core
  Graphics backend + `macos_demo` was implemented and **removed by request**; do
  not reintroduce it without an explicit ask. (Background: `docs/architecture.md`.)
- `cargo bench` uses the `bench` profile (`opt-level = 3`); bench targets use
  `harness = false` and are run via `cargo bench -p <crate> --bench <name>`.
- API priority: **API -> test -> implementation -> integration.**

## Context hygiene (keep agent/LLM context small)

- Do **not** read or `grep` `target/`, `demos/*/dist/` (ignored generated
  wasm/js), or `Cargo.lock`. To find a symbol, `rg` from the repo root (ripgrep
  honors `.gitignore`); avoid `grep -r`.
- Use the map below instead of `ls -R` / `find` exploration.
- Read one module, not a whole crate. If a file passes ~500 lines, prefer
  splitting it over reading it whole.

## Where to look

| I need... | Look at |
|---|---|
| Pipeline, coordinates, stage plan, backend replaceability | `docs/architecture.md` |
| Backends (Canvas / wgpu / recording), adding a backend, browser boundary | `docs/backend.md` |
| Controls, layout, components | `docs/components.md` |
| Design tokens, theme, component library | `docs/design-system.md` |
| Roadmap / remaining primitives & components | `docs/plan.md` |
| Godot-style unified scene migration (Stage 25+) | `docs/godot-migration.md` |
| Profiler + debug overlays | `docs/debug.md` |
| Benchmarks & regression baselines | `docs/benchmarking.md` |
| Test layers, no-screenshot rule | `docs/testing.md` |
| Getting started / build & run | `docs/getting-started.md` |
| Core types & crate APIs | `crates/*/src/*.rs` (module docs at the top) |
