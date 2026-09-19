# quill

A backend-neutral 2D/UI drawing core in Rust, with HTML Canvas 2D (WASM) as the
first render backend. Inspired by Godot's `SceneTree -> Node -> CanvasItem ->
Node2D / Control` model, but with a Rust-friendly API.

## Architecture

```
Input -> SceneTree -> Update -> Layout -> Paint -> DrawList -> RenderBackend -> Pixels
```

| Crate | Responsibility |
|---|---|
| `draw_core` | math, color, IDs, base types |
| `draw_scene` | `Node`, `SceneTree`, `CanvasItem`, `Node2D`, transforms |
| `draw_ui` | `Control`, layout, containers, UI behavior |
| `draw_render` | `DrawCommand`, `DrawList`, `PaintContext`, `RenderBackend` |
| `draw_profile` | frame timing/counters, `Profiler`, `inspect` / `InspectionReport` |
| `draw_debug_ui` | component debug bounds (`DebugOverlay`) + performance panel (`PerformanceOverlay`) |
| `draw_bench` | dependency-free benchmark harness (`BenchRunner`, `Baseline`, regression verdicts) |
| `draw_bench_suite` | deterministic CPU pipeline benchmarks (scene / ui / pipeline) |
| `draw_backend_canvas` | Canvas 2D backend |
| `draw_backend_recording` | headless recording backend for tests |
| `draw_backend_wgpu` | native `wgpu` backend (offscreen, pixel readback) |
| `draw_wasm` | browser glue (events, RAF, canvas wiring) |

Dependency direction is enforced by crate boundaries: the pure core crates never
depend on browser APIs or a concrete backend. See `AGENTS.md`.

## Status

Stage 11 (benchmarking). `draw_core` provides math,
colors, handles and the viewport model; `draw_scene` provides the scene tree
with transform/visibility propagation and a `SceneTree::paint` step;
`draw_render` provides the backend-neutral IR (`DrawCommand`/`DrawList`/
`PaintContext`) and the `RenderBackend` trait; `draw_backend_recording` records
frames for the fully headless `Scene -> DrawList -> RenderBackend` test pipeline;
`draw_backend_canvas` + `draw_wasm` render that IR to an HTML Canvas with DPR
handling and input; `draw_backend_wgpu` renders the same IR with `wgpu` to an
offscreen texture and reads the pixels back for native `cargo test`; `draw_ui`
provides `Control`, layout (anchors/offsets/containers), reusable components
(`Panel`/`VBox`/`HBox`/`Label`/`Button`), hit-tested pointer/keyboard input, and
click callbacks; `draw_profile` records per-phase timings/counters and audits
frames; `draw_ui::paint_debug` + `draw_debug_ui::DebugOverlay` draw yellow
component bounds with `Name #id` labels; and `draw_debug_ui::PerformanceOverlay`
renders the profiler as a toggleable panel.

Three independent renderers consume the same `DrawList`:
`draw_backend_canvas`, `draw_backend_recording`, and `draw_backend_wgpu`.

## Demos

| Demo | Shows |
|---|---|
| `demos/component_demo` | Recommended component API (compose, layout, `on_click`, state, WASM) |
| `demos/web_demo` | Raw scene + UI API and the Canvas backend |
| `demos/wgpu_demo` | Native window + `wgpu` backend (surface presentation) + component/perf debug overlays |

## Build & test

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo bench --workspace --no-run
```

## Benchmark

```bash
cargo bench -p draw_bench_suite                       # CPU pipeline suite
cargo bench -p draw_bench_suite --bench pipeline -- --filter scene/update
cargo bench -p draw_backend_wgpu --bench wgpu         # offscreen + readback (skips with no adapter)
```

Save a baseline and later fail on a regression:

```bash
cargo bench -p draw_bench_suite --bench pipeline -- --save-baseline benches/cpu.txt
cargo bench -p draw_bench_suite --bench pipeline -- --baseline benches/cpu.txt
```

See `docs/benchmarking.md`.

## Run the web demo

```bash
# one-time: install matching tooling
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.128

# build wasm + JS glue into demos/web_demo/dist
./demos/web_demo/build.sh

# serve (ES modules need http, not file://)
python3 -m http.server 8080 --directory demos/web_demo
# open http://localhost:8080/
```

## Run the native wgpu demo

```bash
cargo run -p wgpu_demo --release
```

See `demos/wgpu_demo/README.md` for details. Press **F3** / `` ` `` / **d** to
toggle component debug bounds and **F4** / **p** for the performance panel; see
`docs/debug.md` for wiring them into your own app.
