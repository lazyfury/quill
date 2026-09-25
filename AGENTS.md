# AGENTS.md — quill

Concise working agreement. Details live in `docs/` (see the map at the bottom);
this file is the short source of truth for rules and status.

## Goal

Backend-neutral 2D/UI drawing core in Rust. Godot-inspired:
`SceneTree -> Node -> CanvasItem -> Node2D / Control`.

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
   Exact token names/paths matter: use `theme.palette().*` and
   `theme.surface(level)` rather than hard-coding hex values in components. Dark
   is a token swap, not a second code path, and dark values must stay within the
   documented palette. Density is the same: `theme.density()` (the built-in
   `DefaultTheme::compact()`) changes spacing / control metrics without a second
   code path, and components read
   `theme.spacing`/`control_height`/`row_height` rather than the `space`/`control`
   consts.
   **Migration exception (Stage 25+):** the Godot-style migration
   (`docs/godot-migration.md`) may change `draw_scene` / `draw_ui` incompatible;
   keep the compatibility layer green per phase and update
   `docs/design-system.md` when component-facing APIs move.
9. **Ask when unclear.** If a request is ambiguous, or a change would alter the
   architecture, a public API or the roadmap, stop and ask before writing code;
   do not guess intent.
10. **Roadmap changes need confirmation.** Do not add, remove, reorder or
    re-scope stages/phases (including `docs/godot-migration.md`) without explicit
    approval, and record any approved change in the docs.
11. **Do not start work on your own.** Implement only the agreed task; report
    unrelated findings instead of acting on them.
12. **Accept and commit per user task.** A user-proposed task is reviewed and
    committed as a whole once it is complete — not as agent-internal todo
    subtasks. Present the finished task as a reviewable diff, get approval, then
    commit it (stage reports still follow rule 6).
13. **Correct factual errors.** If a request rests on a factual mistake about
    the code, docs or repo state, say so and give the correct picture before
    acting — do not silently execute it or guess around it.

## Dependency direction

```
draw_core            (no draw_* deps)
draw_theme   -> draw_core
draw_scene    -> draw_core, draw_render
draw_ui         -> draw_core, draw_scene, draw_render
draw_components -> draw_core, draw_scene, draw_render, draw_ui, draw_theme
draw_render   -> draw_core
draw_ffi      -> draw_core, draw_render
                 (C ABI over the core: the value types plus a `DrawList`
                  builder/iterator. No scene/UI and no backend — a
                  foreign-language host organizes its own UI and implements
                  its own backend. Header: `crates/draw_ffi/include/quill.h`.)
demoapp_ffi   -> demo_app, draw_ffi, draw_core, draw_render, draw_theme
                 (C ABI over the *real* `demo_app` gallery: a foreign host
                  creates a DemoApp, drives its frame and reads the resulting
                  DrawList through `draw_ffi`'s command record. The only crate
                  that depends on `demo_app`, which is an example, not a core
                  crate.)
wgpu_ffi      -> draw_backend_wgpu, draw_ffi, draw_core, draw_render
                 (C ABI over the Rust wgpu backend: a foreign host hands over a
                  window view and a DrawList, and the existing backend renders
                  it; a headless handle renders offscreen + reads pixels back.
                  The other side of the C++ demo's backend comparison.)
draw_font     -> draw_core
                 (backend-neutral font service: system-font discovery, family +
                  weight resolution with per-character fallback, `rustybuzz`
                  shaping, `ab_glyph` rasterization into a shared atlas. Owns
                  `ab_glyph` / `rustybuzz` / `ttf-parser` / `memmap2` /
                  `font8x8`.)
draw_svg      -> draw_core, draw_render
                 (backend-neutral SVG vector rendering: parses a small SVG subset
                  into flattened polylines and strokes them with the IR `Line` /
                  `FillCircle` commands — no external dependency, so an icon pack
                  like Lucide can be loaded and drawn by any backend)
draw_assets   -> draw_core
                 (backend-neutral image decode: PNG bytes -> tightly packed RGBA8
                  + dimensions via the pure-Rust `png` crate; a host uploads it
                  through `RenderBackend::register_texture`. No backend/UI/GPU
                  dependency.)
draw_profile  -> draw_core, draw_render
draw_debug_ui -> draw_core, draw_scene, draw_render, draw_ui, draw_components, draw_profile
draw_backend_* -> draw_render, draw_core
draw_wasm     -> draw_render, draw_backend_canvas, draw_core, draw_ui
draw_bench    (std only, no draw_* deps)
draw_bench_suite -> draw_bench, draw_core, draw_render, draw_scene, draw_ui, draw_components
draw_anim     -> draw_core, draw_scene
                 (backend-neutral, time-driven tweens/easing; drives node
                  properties or external values. `is_animating` is the host's
                  "needs another frame" signal. No clock, no backend, no UI.)
draw_game     -> draw_core, draw_render, draw_scene, draw_assets, draw_anim
                 (+ optional draw_ui behind the `ui` feature)
                 (2D game layer: sprites as `Node2D` + `Visual::Sprite`, texture
                  upload through `RenderBackend::register_texture`, sprite-sheet
                  frame animation, lightweight timers and typed signals, AABB/
                  circle collision queries and `Area` enter/exit triggers. The
                  optional `ui` feature adds `GameView`, an embedded sub-viewport
                  Control that renders the world to an offscreen target and
                  composites it. No backend; no rigid bodies, no audio. `quill`'s
                  `game` feature forwards it; `ui`+`game` surfaces `GameView`.)
quill         -> feature-gated re-exports only:
                 `ui`   -> draw_core, draw_render, draw_scene, draw_theme,
                           draw_ui, draw_components
                 `anim` -> draw_anim (+ draw_core, draw_scene)
                 `game` -> draw_game, draw_assets (+ draw_core, draw_render,
                           draw_scene)
                 (application facade; disabled crates are not compiled; `game`
                  does not imply `ui`, `anim` is independent of both. No logic.)
demo_app      -> draw_core, draw_render, draw_scene, draw_ui, draw_components,
                 draw_theme, draw_anim   (no backend; the Animation gallery page
                 drives a draw_anim tween and reports `needs_frame`)
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
cpp_ffi (C++/CMake) -> draw_ffi + demoapp_ffi + wgpu_ffi (staticlibs), GLFW, OpenGL 3.3
                 (standalone C++ host: NOT a Cargo workspace and not a member;
                  `draw_ffi` is the workspace member it links, `demoapp_ffi`
                  lets its `--demoapp` mode load the real gallery, and
                  `wgpu_ffi` gives `--wgpu` the Rust backend as an alternative
                  to the C++ OpenGL one. The UI is C++ — see
                  `examples/cpp_ffi` and `docs/cpp-ffi.md`.)
```

`image_editor` (the Photoshop-style app) used to live in `examples/image_editor`:
it graduated to its own repo (a sibling checkout, `../image_editor`) and consumes
these crates through relative path deps, so it is no longer part of this
checkout.

Planned (Stages 28-31, see `docs/godot-migration.md`):

```
examples/game_demo -> draw_game, draw_scene, draw_anim, one backend  # Stage 30
```

The core crates stay fine-grained on purpose; applications use the `quill`
facade with opt-in features (`ui`, `anim`, `game`, `wgpu`, `canvas`, `wasm`,
`profile`, `debug`, `recording`, `bench`). A UI-only app must not compile
`draw_game`; `anim` is independent of `game`.

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
(`ab_glyph`), text shaping (`rustybuzz`, `unicode-bidi`) and system-font
discovery live only in `draw_font` (which `draw_backend_wgpu` consumes); the
core stays text-free.

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
| `examples/cpp_ffi` | **standalone** (C++/CMake; no Cargo workspace) | C++17 UI + OpenGL 3.3 backend; links `draw_ffi` + `demoapp_ffi` + `wgpu_ffi` | `./examples/cpp_ffi/build.sh`; `./examples/cpp_ffi/build/cpp_ffi --selfcheck` (`--dump`, `--gallery`, `--demoapp`, `--wgpu` too) | `examples/cpp_ffi/README.md`, `docs/cpp-ffi.md` |
| `examples/demoapp_ffi` | root member | `staticlib`/`cdylib` C ABI over `demo_app`; loaded by `cpp_ffi --demoapp` | `cargo test -p demoapp_ffi` | `docs/cpp-ffi.md` |
| `examples/wgpu_ffi` | root member | `staticlib`/`cdylib` C ABI over `draw_backend_wgpu`; `cpp_ffi --wgpu` (and `--selfcheck --wgpu`) | `cargo test -p wgpu_ffi` | `docs/cpp-ffi.md` |

Headless self-check binaries (`--selfcheck`, and `--dump*` where noted) render
the same UI into `draw_backend_recording` and print a report; they are the
no-screenshot verification for the standalone hosts. Workspace members are
covered by the normal `cargo test --workspace` gate. `cpp_ffi` is the exception:
it has no recording backend, so its `--selfcheck` reads its own OpenGL
framebuffer back with `glReadPixels`, and `--dump` prints the `DrawList` command
stream with no GPU at all.

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

All stages through **Stage 29 are complete and accepted.** The full ledger (one
line per stage, with what each landed) is `docs/architecture.md` →
"Implementation stages"; the Godot-style migration's phase plan and per-substage
notes are `docs/godot-migration.md`.

- **Current status:** Stage 29 (`GameView`/sub-viewport + fixed timestep)
  accepted — 29.1 `SceneTree::physics_process`, 29.2 `RenderTargetId` +
  render-target contract (wgpu/recording), 29.3 `draw_game::GameView` (behind the
  optional `ui` feature), 29.4 `FixedTimestep` clock. Previously: Stage 28
  (`draw_game` 2D game layer), Stage 27 (refresh decoupling), Stage 26
  (`draw_anim` + `quill` facade skeleton), Stage 25 (Godot-style unified scene),
  `draw_font`, `Theme` trait.
- **Next (future stages):** Stages 30-31 — `examples/game_demo`, remaining
  `quill` facade backend features. Phase 8 observability remains.
- **Current stage:** none — next up Stage 30 (`examples/game_demo`).

On acceptance of a whole user task, the agent writes the durable summary into
this file (the "Current stage" bullet under "Stages" plus any doc updates).

## Per-stage gate (must run)

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo bench --workspace --no-run
```

`--workspace` excludes the **standalone** demos (`examples/deepseek_balance`,
`examples/file_browser`). When you touch one, also run its own gate with
`--manifest-path` (fmt / check / test) and its `--selfcheck`; see "Demo workspace
modes & how to test" above. `image_editor` now lives in its own repo
(`../image_editor`); changes there have their own gate.

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
| C ABI, C++ host, foreign-language backend | `docs/cpp-ffi.md` (`crates/draw_ffi`, `examples/cpp_ffi`) |
| Fonts: discovery, family/weight resolution, fallback, shaping | `docs/font.md` (`crates/draw_font`) |
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
