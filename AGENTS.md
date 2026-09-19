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
5. No ECS, shaders, render graph, particles, physics, editor in MVP.
6. Do not merge stages. Each stage ends with a report and waits for user approval.
7. **No screenshot / screen-recording visual testing.** Never use
   `screencapture`, browser screenshots, screen recording, or any OS-level
   capture to verify rendering. Verify programmatically instead: read the
   backend's own pixel buffer, assert `DrawList` command sequences, or read DOM
   state markers. If a claim cannot be verified without a screenshot, say so
   rather than capturing one.

## Dependency direction

```
draw_core            (no draw_* deps)
draw_scene    -> draw_core, draw_render
draw_ui       -> draw_core, draw_scene, draw_render
draw_render   -> draw_core
draw_profile  -> draw_core, draw_render
draw_debug_ui -> draw_core, draw_render, draw_ui, draw_profile
draw_backend_* -> draw_render, draw_core
draw_wasm     -> draw_render, draw_backend_canvas, draw_core
draw_bench    (std only, no draw_* deps)
draw_bench_suite -> draw_bench, draw_core, draw_render, draw_scene, draw_ui
web_demo      -> draw_core, draw_render, draw_scene, draw_wasm
wgpu_demo     -> draw_core, draw_render, draw_scene, draw_ui, draw_backend_wgpu,
                 draw_profile, draw_debug_ui, winit
```

`draw_scene -> draw_render` is intentional: `draw_render` is the backend-neutral
IR (no backend/browser deps), and the Paint step (Scene -> DrawList) lives in the
scene. This does not weaken backend replaceability.

Browser APIs only in `draw_backend_canvas`, `draw_wasm`, `demos/web_demo`.
`winit` only in `demos/wgpu_demo`. `wgpu` only in `draw_backend_wgpu` (plus its
tests/bench) and `demos/wgpu_demo`.

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
| Profiler + debug overlays | `docs/debug.md` |
| Benchmarks & regression baselines | `docs/benchmarking.md` |
| Test layers, no-screenshot rule | `docs/testing.md` |
| Getting started / build & run | `docs/getting-started.md` |
| Core types & crate APIs | `crates/*/src/*.rs` (module docs at the top) |
