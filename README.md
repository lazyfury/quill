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
| `draw_backend_canvas` | Canvas 2D backend |
| `draw_backend_recording` | headless recording backend for tests |
| `draw_wasm` | browser glue (events, RAF, canvas wiring) |

Dependency direction is enforced by crate boundaries: the pure core crates never
depend on browser APIs or a concrete backend. See `AGENTS.md`.

## Status

Stage 8 (second backend: macOS Core Graphics). `draw_core` provides math, colors, handles and
the viewport model; `draw_scene` provides the scene tree with transform/visibility
propagation and a `SceneTree::paint` step; `draw_render` provides the
backend-neutral IR (`DrawCommand`/`DrawList`/`PaintContext`) and the
`RenderBackend` trait; `draw_backend_recording` records frames for the fully
headless `Scene -> DrawList -> RenderBackend` test pipeline; `draw_backend_canvas`
+ `draw_wasm` render that IR to an HTML Canvas with DPR handling and input; and
`draw_ui` provides `Control`, layout (anchors/offsets/containers), reusable
components (`Panel`/`VBox`/`HBox`/`Label`/`Button`), hit-tested pointer/keyboard
input, and click callbacks. `draw_backend_coregraphics` renders the same
`DrawList` natively on macOS via Core Graphics / Core Text.

## Demos

| Demo | Shows |
|---|---|
| `demos/component_demo` | Recommended component API (compose, layout, `on_click`, state, WASM) |
| `demos/web_demo` | Raw scene + UI API and the Canvas backend |
| `demos/macos_demo` | Native Core Graphics backend: offscreen PNG + AppKit window |

## Build & test

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
```

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

## Run the macOS demo

```bash
# macOS only
./target/debug/macos_demo --offscreen /tmp/quill.png   # headless PNG, exits
./target/debug/macos_demo                              # AppKit window
./target/debug/macos_demo --selftest                   # render a window frame, assert pixels, exit
```
