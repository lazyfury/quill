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

Stage 25 (Godot-style unified scene) is accepted: one `SceneTree` owns world
(`Node2D`) and UI (`Control`), with `draw_scene::{Viewport, Camera2D,
CanvasLayer}` and the `Scene -> DrawList` paint step. Three independent renderers
consume the same backend-neutral `DrawList` — Canvas 2D (WASM), the headless
recording backend, and native `wgpu` (offscreen + pixel readback). See
[`docs/architecture.md`](docs/architecture.md) for the stage ledger and
[`docs/getting-started.md`](docs/getting-started.md) to build an app.

## Examples

| Example | Shows |
|---|---|
| `examples/demo_app` | Shared three-column, macOS-style notes app (backend-neutral `DemoApp`) |
| `examples/web_demo` | `demo_app` on the Canvas 2D backend (`draw_wasm`) |
| `examples/wgpu_demo` | `demo_app` on a native `wgpu` surface + component/perf debug overlays |
| `examples/multi_tree` | Headless: repeated `into_tree()` calls yield independent trees (no shared ids/state) |
| `examples/deepseek_balance` | Standalone macOS menu-bar tool: DeepSeek balance panel built from `draw_theme`/`draw_components` on a transparent `wgpu` surface |

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
