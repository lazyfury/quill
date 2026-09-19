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
| `draw_render` | `DrawCommand`, `DrawList`, `PaintContext`, `RenderBackend` |
| `draw_scene` | `Node`, `SceneTree`, `CanvasItem`, `Node2D`, transforms, `Viewport`/`Camera2D` |
| `draw_ui` | `Control` runtime, layout, paint and input routing |
| `draw_theme` | design tokens (palette, spacing, radius, type, motion) |
| `draw_components` | component library: base builders (`Component`/`Spec`) + themed components |
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

Stage 25 (Godot-style unified scene). One `SceneTree` owns world (`Node2D`) and
UI (`Control`); `draw_scene` provides `Viewport` / `Camera2D` / `CanvasLayer` and
the `Scene -> DrawList` paint step; `draw_render` is the backend-neutral IR
(`DrawCommand`/`DrawList`/`PaintContext`) plus the `RenderBackend` trait;
`draw_ui` owns the `Control` runtime, layout, paint and input routing;
`draw_theme` provides design tokens and `draw_components` the component library
(base builders + themed components); `draw_backend_recording` gives the fully
headless `Scene -> DrawList -> RenderBackend` test path, `draw_backend_canvas` +
`draw_wasm` render to an HTML Canvas (DPR + input), and `draw_backend_wgpu`
renders the same IR offscreen and reads pixels back for native `cargo test`.
`draw_profile` records per-phase timings and audits frames;
`draw_debug_ui::DebugOverlay` / `PerformanceOverlay` draw component bounds and the
profiler panel.

Three independent renderers consume the same `DrawList`:
`draw_backend_canvas`, `draw_backend_recording`, and `draw_backend_wgpu`.

## Examples

| Example | Shows |
|---|---|
| `examples/demo_app` | Shared three-column, macOS-style notes app (backend-neutral `DemoApp`) |
| `examples/web_demo` | `demo_app` on the Canvas 2D backend (`draw_wasm`) |
| `examples/wgpu_demo` | `demo_app` on a native `wgpu` surface + component/perf debug overlays |

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
cargo bench -p draw_bench_suite --bench pipeline -- --save-baseline benches/cpu.baseline.txt
cargo bench -p draw_bench_suite --bench pipeline -- --baseline benches/cpu.baseline.txt
```

See `docs/benchmarking.md`.

## Run the web demo

```bash
# one-time: install matching tooling
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.128

# build wasm + JS glue into examples/web_demo/dist
./examples/web_demo/build.sh

# serve (ES modules need http, not file://)
python3 -m http.server 8080 --directory examples/web_demo
# open http://localhost:8080/
```

## Run the native wgpu demo

```bash
cargo run -p wgpu_demo --release
```

See `examples/wgpu_demo/README.md` for details. Press **F3** / `` ` `` / **d** to
toggle component debug bounds and **F4** / **p** for the performance panel; see
`docs/debug.md` for wiring them into your own app.
