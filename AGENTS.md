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
   Density is the same: `Theme.density` (`compact()`) changes spacing / control
   metrics without a second code path, and components read
   `theme.spacing`/`control_height`/`row_height` rather than the `space`/`control`
   consts.
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
draw_components -> draw_core, draw_scene, draw_render, draw_ui, draw_theme
draw_render   -> draw_core
draw_svg      -> draw_core, draw_render
                 (backend-neutral SVG vector rendering: parses a small SVG subset
                  into flattened polylines and strokes them with the IR `Line` /
                  `FillCircle` commands — no external dependency, so an icon pack
                  like Lucide can be loaded and drawn by any backend)
draw_profile  -> draw_core, draw_render
draw_debug_ui -> draw_core, draw_scene, draw_render, draw_ui, draw_components, draw_profile
draw_backend_* -> draw_render, draw_core
draw_wasm     -> draw_render, draw_backend_canvas, draw_core, draw_ui
draw_bench    (std only, no draw_* deps)
draw_bench_suite -> draw_bench, draw_core, draw_render, draw_scene, draw_ui, draw_components
demo_app      -> draw_core, draw_render, draw_scene, draw_ui, draw_components,
                 draw_theme   (no backend)
web_demo      -> draw_core, draw_scene, demo_app, draw_wasm
multi_tree    -> draw_core, draw_render, draw_scene, draw_ui, draw_components,
                 draw_theme, draw_backend_recording  (headless, no window host)
wgpu_demo     -> draw_core, draw_render, draw_scene, draw_ui, demo_app,
                 draw_backend_wgpu, draw_profile, draw_debug_ui, winit
deepseek_balance -> draw_core, draw_render, draw_scene, draw_theme, draw_ui,
                 draw_components, draw_backend_wgpu, winit, ureq,
                 deepseek_util (own sub-crate `examples/deepseek_balance/util`:
                 time + currency helpers, std-only)
                 (standalone tool: own workspace, NOT a workspace member,
                  so it stays out of `cargo check --workspace`)
file_browser   -> draw_core, draw_render, draw_scene, draw_theme, draw_ui,
                 draw_components, draw_backend_wgpu, draw_backend_recording,
                 draw_profile, winit
                 (standalone demo: own workspace, NOT a workspace member;
                  the first real consumer of `draw_components::List`, and the
                  first host to translate a platform wheel into
                  `InputEvent::Wheel`)
image_editor   -> draw_core, draw_render, draw_scene, draw_theme, draw_ui,
                 draw_components, draw_svg, draw_backend_wgpu, draw_backend_recording,
                 draw_profile, winit, tracing
                 (standalone demo: own workspace, NOT a workspace member;
                  the Photoshop-style editor demo whose UI is built with the
                  quill stack instead of egui. Landed: menu/toolbar/tool-options
                  bar/status bar, a resizable right sidebar whose file/layer/
                  properties sections are split by `ResizeHandle::horizontal`
                  dividers (`ResizeHandle`),
                  `document` model (`Document`/`Layer`/`PixelBuffer`, plus
                  `PixelRegion`), `canvas` (a `Node2D` + `Visual::Image`, camera
                  zoom/pan, coordinate conversion), `renderer` (CPU compositor),
                  a working layer panel, `tools` (`BrushTool` paint/erase in the
                  active layer), `history` (`Command`/`History`: one stroke = one
                  undo, and layer edits are undoable too — paint, move
                  (`SetLayerPositionCommand`), add/remove, rename/visibility/
                  opacity/order (`LayerMetaCommand`), crop; toolbar buttons +
                  `Ctrl/Cmd+Z`), and Lucide icons (toolbar tool + undo/redo
                  buttons) built as an `Icon` component that strokes straight
                  into the IR via `draw_svg` (no rasterization, no texture;
                  vendored 7-icon subset matching the toolbar +
                  `IMAGE_EDITOR_ICON_DIR` to point at a full pack)), and PNG import/export (`io` `codec`/`file` on the
                  `png` crate — a dependency only in this example, the core
                  stays dependency-free; a sidebar `file` panel with inline
                  path editing since winit has no native file dialog), and the
                  move / rectangle-select / eyedropper tools (`MoveTool` moves a
                  layer's `position`; selection lives in `AppState` and clips the
                  brush via `canvas::pixel_selection`; the eyedropper reads the
                  composite through `renderer::sample_pixel`), and a real menu
                  bar (a title click opens `draw_components::Overlays::menu`,
                  whose content is a `Menu` of `MenuItem`s; undo/redo, import/
                  export, zoom/fit, clear-selection and about are wired, the rest
                  are labeled placeholders; the reusable `Menu`/`MenuItem` live
                  in `draw_components`), and a compact custom theme
                  (`theme::editor_theme`, `Density::COMPACT`). The default canvas
                  is a 128×128 pixel-art document with a **pixel mode**
                  (`BrushTool.hard` + `BrushShape::{Round, Square}`, both toggled
                  from the tool-options bar: hard edges at any size, snapped to the
                  pixel grid, a Bresenham 1px line, and a screen-space pixel grid
                  overlay above `zoom >= 6`), displayed with nearest-neighbour
                  texture filtering
                  (`WgpuBackend::set_texture_filter` + `TextureFilter::Nearest`)
                  and a checkerboard transparency backdrop (toggled from the 视图
                  menu). The move tool drags a
                  layer's `position` (its pixel-buffer origin, possibly negative);
                  the first brush stroke calls
                  `Document::ensure_layer_covers_document`, which grows the buffer to
                  the union of its extent and the document, so strokes land under
                  the cursor, the vacated document area stays drawable, and pixels
                  moved off-canvas are kept (not cropped) — the history regions are
                  shifted to match; the 图层 menu's 「裁到文档」 reclaims the grown
                  buffer. Verified headlessly
                  with `--selfcheck` (undo/redo, a real export->decode + import
                  round-trip, the three tools, the menu open -> item -> close
                  loop, and an icon/FillCircle check))
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

Browser APIs only in `draw_backend_canvas`, `draw_wasm`, and the WASM example
(`examples/web_demo`).
`winit` only in the window hosts: `examples/wgpu_demo` and the standalone,
non-member `examples/deepseek_balance` and `examples/file_browser` tools (their
UI is built from `draw_theme` / `draw_components` / `draw_ui`; blocking work —
the network call, the directory scan — runs on a worker thread and comes back
through a winit `EventLoopProxy`). `wgpu` only in
`draw_backend_wgpu` (plus its tests/bench) and those window hosts. Font parsing
(`ab_glyph`), text shaping
(`rustybuzz`, `unicode-bidi`) and system-font discovery live only in
`draw_backend_wgpu`; the core stays text-free.

## Demo workspace modes & how to test

Root `cargo check --workspace` / `cargo test --workspace` only cover the
**workspace members** below. The **standalone** demos are deliberately kept out
of the root workspace (their `winit` / `wgpu` / `ureq` / `png` deps must not
enter the core gate), so they are built and tested with `--manifest-path`.
None of the demos is a dependency of the core crates.

| Demo | Workspace mode | Scope | Build / test | Reference |
|---|---|---|---|---|
| `examples/demo_app` | root member | single crate, backend-neutral (no backend) | `cargo test -p demo_app` | dependency block above |
| `examples/multi_tree` | root member | single crate, headless (`draw_backend_recording`) | `cargo test -p multi_tree` | dependency block above |
| `examples/web_demo` | root member | WASM / Canvas host | `cargo test -p web_demo`; build `./examples/web_demo/build.sh` | `examples/web_demo/README.md` |
| `examples/wgpu_demo` | root member | native `wgpu` + `winit` | `cargo test -p wgpu_demo`; run `cargo run -p wgpu_demo --release` | `examples/wgpu_demo/README.md`, `docs/debug.md` |
| `examples/deepseek_balance` | **standalone** (own workspace) | own `util` sub-crate (member of that workspace); native `wgpu` + `winit` + `ureq` | `cargo test --manifest-path examples/deepseek_balance/Cargo.toml`; `cargo run --manifest-path examples/deepseek_balance/Cargo.toml -- --selfcheck` | dependency block above, crate module docs |
| `examples/file_browser` | **standalone** (own workspace) | single crate; native `wgpu` + `winit` | `cargo test --manifest-path examples/file_browser/Cargo.toml`; `cargo run --manifest-path examples/file_browser/Cargo.toml -- --selfcheck` (`--dump` too) | dependency block above |
| `examples/image_editor` | **standalone** (own workspace) | single crate; native `wgpu` + `winit` + `png` + `tracing`; vendored SVG icons | `cargo test --manifest-path examples/image_editor/Cargo.toml`; `cargo run --manifest-path examples/image_editor/Cargo.toml -- --selfcheck` | `examples/image_editor/README.md`, `examples/image_editor/todo.md` |

Headless self-check binaries (`--selfcheck`, and `--dump*` where noted) render
the same UI into `draw_backend_recording` and print a report; they are the
no-screenshot verification for the standalone hosts. Workspace members are
covered by the normal `cargo test --workspace` gate.

## Code division (one concern per module)

Modules are cut by concern, not by size and not by convenience. A file that
passes ~500 lines gets split along its seams, and a concern that is true of the
domain — not of HTTP, not of the UI, not of the platform — gets its own small
crate/module instead of being embedded where it happens to be used.

- **Name the owner.** Each layer has one job: the API module owns HTTP + wire
  parsing, the view module owns tree construction, the state machine owns
  decisions, the host owns the platform loop. Formatting is not the API's job;
  parsing is not the UI's.
- **Generic helpers get a home.** Time formatting/parsing, currency rendering
  and the like belong in a dependency-free helper crate/module, split by
  concern (`time.rs`, `currency.rs`, …), so they are testable and reusable and
  the callers stay focused. Do not scatter them across the files that use them.
- **A standalone example keeps its own sub-crate.** `examples/deepseek_balance`
  is its own workspace; its helpers live in `examples/deepseek_balance/util`
  (member of that workspace) so they never leak into `cargo check --workspace`.
- **Respect the dependency direction** above: a helper crate depends on nothing
  backend- or UI-specific; helpers never import the layer that consumes them.
- **Tests live with the concern.** A moved function takes its tests with it;
  don't pile every test into one file or keep tests for code that moved.

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
- [x] Stage 18 — shared `examples/demo_app` used by `wgpu_demo` and the WASM demos,
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
      `SceneTree::add_child`; `draw_components::Component` carries a `Spec` and exposes
      modifiers as methods; `draw_components` components take the `Theme` as a
      `Copy` value; the theme is no longer stored on the tree.
      **Stage 25.12 (`Line` primitive):** `DrawCommand::Line { from, to, paint,
      width }` + `PaintContext::draw_line`, implemented in Canvas / wgpu /
      recording; `Divider` and column separators draw a real line.
      **Stage 25.13 (drag + resize):** `GuiState.dragging` / `Control.drag_callback`
      with pointer capture in `draw_ui::handle_input`; `Component::on_drag`
      (`DragPhase::{Start,Move,End}` + delta) / `draw_components::set_on_drag`;
      `draw_components::ResizeHandle` (a divider-styled gutter that resizes a
      target pane's flex basis). `draw_core::Cursor` + `ControlData.cursor` +
      `Component::dynamic_cursor` (per-control provider) +
      `draw_ui::hovered_cursor`; hosts map it (winit `CursorIcon`, canvas CSS
      `cursor`). `demo_app`'s sidebar and list gutters are both draggable.
      **Stage 25.14 (clip + wheel + `List`):** `ControlData.clip` (opt-in, the
      only source of `DrawCommand::ClipRect`; resolved per layout pass into
      `ControlData.clip_rect`, intersected with the nearest clipping ancestor) +
      `Ui::paint` emitting one save/clip/restore per clipped region +
      clip-aware hit testing; `InputEvent::Wheel` routing in
      `draw_ui::handle_input` to the nearest `Control::scroll_callback`
      (`draw_components::set_on_scroll` / `Component::on_scroll`); and
      `draw_components::{List, ListState, ListColumn, RowSource}` — a
      virtualized list whose frame cost is flat in the row count (107 controls /
      72 commands per frame at 1 K, 10 K and 100 K rows; `docs/benchmarking.md`).
      Additive to the frozen core: no `Widget` variant, existing `ControlData`
      fields unchanged. Recorded in `docs/design-system.md`.
      **Stage 25.14 demo:** `examples/file_browser` (own workspace) is the first
      real consumer of `List` and the first host that turns a platform wheel into
      `InputEvent::Wheel` (`host::wheel_pixels`); it scans directories on a worker
      thread and verifies itself headlessly with `--selfcheck` / `--dump`.
   - **Stage 25.15 (resizable split + binary preview, demo layer only — no core
     change):** `examples/file_browser` splits into two panes the way
     `demo_app` does — `Flex::row()` of `main(basis Px) |
     ResizeHandle::vertical(theme).target(main) | preview(grow 1)`, so the one
     gutter drives the left pane's basis and the right pane takes the rest.
     Two things the component cannot do for you: a handle's `min`/`max` are
     fixed at build time and know nothing about the viewport, so
     `Browser::layout` re-clamps the main width to
     `viewport - PREVIEW_MIN - gutter` every frame (otherwise a narrow window
     squeezes the right pane to zero); and each virtualized list needs its own
     `ListState::sync` in that same three-step frame. The right pane is a second
     `List` over the selected file's first 64 KiB — 4096 rows of data, ~30 rows
     mounted — formatted by `examples/file_browser/src/preview.rs` (pure
     offset/hex/ascii functions) and read on a worker thread with the same
     generation guard as the directory scan, so sweeping the selection with the
     arrow keys leaves exactly one request in flight.
   - **Stage 25.16 (preview mode, demo layer only — no core change):** the right
     pane now shows those bytes two ways — `PreviewMode::Binary` (offset/hex/ascii)
     and `PreviewMode::Text` (line number + line content) — toggled with `T` or by
     clicking the pane's two tab buttons. The bytes are read once; the mode only
     changes how a row is computed, so switching is free and it survives selecting
     another file. The two modes are two `List`s (a list's columns are fixed at
     build time) chosen by `SceneTree::set_visible`: a hidden list's container has
     zero height, so `ListState::sync` returns early and it owns no row pool at
     all — the idle mode costs nothing. Tabs are
     `Flex::row().on_click(..).dynamic_background(..)`, so the active one is
     highlighted without rebuilding the tree, and the click only writes a shared
     cell that `Browser::update` drains (`on_click` cannot borrow the view).
     Known gap: `draw_ui`'s word-based wrapping collapses leading whitespace, so
     text mode cannot show indentation — `docs/plan.md` tracks it.

## Per-stage gate (must run)

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo bench --workspace --no-run
```

`--workspace` excludes the **standalone** demos (`examples/deepseek_balance`,
`examples/file_browser`, `examples/image_editor`). When you touch one, also run
its own gate with `--manifest-path` (fmt / check / test) and its `--selfcheck`;
see "Demo workspace modes & how to test" above.

Then emit the report and stop for approval.

## Recurring decisions (do not undo)

- Stage 8: the second backend is `draw_backend_recording`. A native macOS Core
  Graphics backend + `macos_demo` was implemented and **removed by request**; do
  not reintroduce it without an explicit ask. (Background: `docs/architecture.md`.)
- `cargo bench` uses the `bench` profile (`opt-level = 3`); bench targets use
  `harness = false` and are run via `cargo bench -p <crate> --bench <name>`.
- API priority: **API -> test -> implementation -> integration.**

## Context hygiene (keep agent/LLM context small)

Learned the hard way: a few sessions ballooned past 150k tokens mostly from
re-reading two 2k-line files and re-printing `--dump`.

- Do **not** read or `grep` `target/`, `examples/*/dist/` (ignored generated
  wasm/js), or `Cargo.lock`. Use `rg` from the repo root to locate a symbol
  **before** opening a file (ripgrep honors `.gitignore`); avoid `grep -r`.
- Use the map below instead of `ls -R` / `find` exploration.
- Read one module, not a whole crate. If a file passes ~500 lines, split it
  instead of reading it whole; read only the window around the change.
- For "where is X / how does X work", delegate to the `explore` subagent and ask
  for a short answer with `file:line` — raw file contents should not enter the
  main context.
- Keep command noise out: capture `cargo` / `--dump` output to a file or pipe it
  through `rg`/`sed`. Never print a whole UI tree or command list; add a filter
  flag to the dump tool rather than dumping everything.
- Batch verification into one call (`cargo fmt --check && cargo test && <selfcheck>`),
  not one command per concern.
- Do not re-read a file after editing it: the `edit` tool matches unique
  surrounding context and needs no fresh read. Never `git stash`/`pop` just to
  diff a revision — use `git show HEAD:path > /tmp/x` or a worktree.
- When a change spans many call sites (a refactor), rewrite the module in one
  pass and compile per layer (API -> impl -> host) so errors stay local.
- Keep web search cheap: few results, small context window.

## Test discipline (keep the suite high-signal)

The suite is a contract, not a diary. Before adding or keeping a test:

- One behaviour per test, named as the rule. Merge assertions that only make
  sense together (parse + fields, show + hide).
- Do not re-test a shared gate through every entry point. The throttle / refresh
  intent is one rule: test it once, not separately for click, key and timer
  rejection.
- Turn several near-identical cases into one table/loop.
- Keep negative controls (tests that prove a checker actually fires) and any
  test whose comment records a past bug — those are not redundant.
- `--selfcheck` already frame-checks layouts; do not duplicate it with
  hand-rolled draw-list assertions unless the check is new.
- If the count outgrows the behaviour it covers, delete before adding. See
  `docs/testing.md` for the layers.

## Where to look

| I need... | Look at |
|---|---|
| **Build an app UI: frame loop, widgets, hosting, conventions, cheat sheet** | **`docs/ui-guide.md`** (read this before scanning crates) |
| Pipeline, coordinates, stage plan, backend replaceability | `docs/architecture.md` |
| Backends (Canvas / wgpu / recording), adding a backend, browser boundary | `docs/backend.md` |
| SVG / vector icons, loading an icon pack (Lucide) | `docs/svg.md` (`crates/draw_svg`) |
| Controls, layout, components (API reference by name) | `docs/components.md` |
| Design tokens, theme, component library | `docs/design-system.md` |
| Roadmap / remaining primitives & components | `docs/plan.md` |
| Godot-style unified scene migration (Stage 25+) | `docs/godot-migration.md` |
| Profiler + debug overlays | `docs/debug.md` |
| Benchmarks & regression baselines | `docs/benchmarking.md` |
| Test layers, no-screenshot rule | `docs/testing.md` |
| Getting started / build & run | `docs/getting-started.md` |
| Core types & crate APIs | `crates/*/src/*.rs` (module docs at the top) |
