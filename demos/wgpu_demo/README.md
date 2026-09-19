# wgpu demo

Native window demo for the `draw_backend_wgpu` backend.

It opens a `winit` window, renders the same `SceneTree` + `Ui` the web demos
use through `WgpuBackend`, and presents the result to a `wgpu` surface:

```text
winit events -> InputEvent -> Scene/UI -> DrawList -> WgpuBackend -> surface
```

No core, scene, UI or IR code changes between the Canvas backend and this one.

## Run

```bash
cargo run -p wgpu_demo --release
```

## Controls

- The rectangle in the scene rotates continuously.
- Click the **Click me** button in the right-hand panel; the status label
  updates the click count.
- Resize the window (the UI re-lays out) or move it between displays with
  different DPRs.

### Debug shortcuts

| Keys | Toggles |
|---|---|
| **F3** / `` ` `` / **d** | Component debug bounds — a yellow border + `Name #id` on every control |
| **F4** / **p** | Performance panel |
| **F5** / **o** | Profiler (record on/off) |

> On macOS the top-row F-keys are often system keys (Mission Control, Spotlight,
> ...). Hold **Fn** or use the `` ` `` / **d** / **p** / **o** fallbacks.

## Command-line options

| Flag | Default | Effect |
|---|---|---|
| `--debug-ui` / `--debug` | on | Draw component bounds at startup |
| `--no-debug-ui` / `--no-debug` | | Start without component debug drawing |
| `--performance` / `--perf` | off | Show the performance panel |
| `--no-performance` / `--no-perf` | | Hide the performance panel |
| `--profiler` / `--profile` | on | Collect frame stats into the profiler |
| `--no-profiler` / `--no-profile` | | Disable the profiler (the panel shows placeholders) |
| `-h`, `--help` | | Print help and exit |
| `-V`, `--version` | | Print the version and exit |

Examples:

```bash
# default: component bounds on, performance panel off
cargo run -p wgpu_demo --release

# component bounds + performance panel
cargo run -p wgpu_demo --release -- --performance

# window only, no overlay and no audit cost
cargo run -p wgpu_demo --release -- --no-debug-ui --no-profiler
```

## Component debug drawing

Every visible `Control` is outlined in yellow with a `Name #id` label at its
top-left corner. This is `draw_ui::Ui::paint_debug` wrapped by
`draw_debug_ui::DebugOverlay`:

```text
demo.paint -> debug.paint(&demo.ui(), ctx) -> `ui.paint_debug(...)`
```

See `docs/debug.md` for the options (`show_names`, `show_ids`, colors, ...) and
for using it with your own `Ui`.

## Performance panel

The demo instruments each pipeline phase, records it in a
`draw_profile::Profiler`, audits the frame's `DrawList` with
`draw_profile::inspect`, and renders the result with
`draw_debug_ui::PerformanceOverlay`:

```text
update/layout/paint/render (timed) -> Profiler.record -> inspect -> PerformanceOverlay -> DrawList
```

The panel shows FPS, current frame time (avg/max), profiler state (`on` /
`paused`), per-phase averages, command counts (current/peak), node & control
counts, inspection findings, and a key legend footer. Panel input is consumed
over the panel; everything else is forwarded to the demo.

## Notes

- Rendering is animated with `winit`'s `ControlFlow::Poll` and
  `Window::request_redraw`; no timer thread is used.
- The surface format prefers a non-sRGB format so shader output matches the
  Canvas backend; if only sRGB is offered, that is used as a fallback.
- The backend is window-agnostic: it receives a surface texture view via
  `WgpuBackend::begin_frame_with_view` and the demo calls `present()`. The same
  backend runs headlessly in `crates/draw_backend_wgpu/tests/render.rs`.
