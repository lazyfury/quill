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

- A three-column, macOS-style notes app built from `draw_components` components on the
  `draw_ui` core:
  - **sidebar** (220px): traffic lights, app title, search placeholder, nav
    rows with selection, version badge,
  - **content list** (324px): header, note rows with thumbnail placeholders,
    selection highlight and an accent bar,
  - **detail**: toolbar with icon buttons, a static image placeholder
    (monochrome rounded square), title/metadata, wrapping body text, preference
    controls and action buttons.
- Icons and images are monochrome rounded-square placeholders.
- Clicking a note row (or the back/forward icon buttons) updates the detail
  pane; clicking nav rows updates the sidebar selection; **New Note** increments
  the click counter (the headless probe).
- Resize the window (the UI re-lays out) or move it between displays with
  different DPRs.

### Rendering

The app is static and the window is **event-driven**: it renders on input,
resize, scale and overlay toggles, then blocks on `ControlFlow::Wait`. There is
no idle redraw loop, so a stationary window uses ~0% CPU. Overlays (debug /
performance) refresh when you interact or move the pointer rather than every
frame.

The demo measures text with the backend's actual loaded font
(`WgpuBackend::text_metrics`), so wrapping and advances stay in sync with what
is rendered. System glyphs are rasterized at device pixels, so text stays crisp
on HiDPI displays.

### Fonts

The backend font is configurable:

- **System** (default): loads `QUILL_FONT` or a per-OS font (proportional Latin
  + CJK), rasterized at device pixels for HiDPI.
- **Pixel**: the built-in `font8x8` bitmap (the original pixel look).

Switch at runtime with **f**, or start in pixel mode with `--pixel-font`.

### Debug shortcuts

| Keys | Toggles |
|---|---|
| **F3** / `` ` `` / **d** | Component debug bounds — a yellow border + `Name #id` on every control |
| **F4** / **p** | Performance panel |
| **F5** / **o** | Profiler (record on/off) |
| **f** | Pixel / system font |

> On macOS the top-row F-keys are often system keys (Mission Control, Spotlight,
> ...). Hold **Fn** or use the `` ` `` / **d** / **p** / **o** fallbacks.

## Command-line options

| Flag | Default | Effect |
|---|---|---|
| `--debug-ui` / `--debug` | | Draw component bounds at startup |
| `--no-debug-ui` / `--no-debug` | on | Start without component debug drawing |
| `--performance` / `--perf` | | Show the performance panel |
| `--no-performance` / `--no-perf` | on | Hide the performance panel |
| `--profiler` / `--profile` | on | Collect frame stats into the profiler |
| `--no-profiler` / `--no-profile` | | Disable the profiler (the panel shows placeholders) |
| `--pixel-font` / `--pixel` | | Use the built-in pixel font instead of a system font |
| `--system-font` / `--smooth-font` | on | Use a system font (falls back to pixel if none loads) |
| `-h`, `--help` | | Print help and exit |
| `-V`, `--version` | | Print the version and exit |

Examples:

```bash
# default: no overlays (press d / p at runtime to show them)
cargo run -p wgpu_demo --release

# component bounds only
cargo run -p wgpu_demo --release -- --debug-ui

# component bounds + performance panel
cargo run -p wgpu_demo --release -- --debug-ui --performance

# window only, no overlays and no audit cost
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

- The scene/UI/layout code lives in the shared, backend-neutral
  `demos/demo_app` crate and is also used by the WASM (`web_demo`) demo; this
  binary only adds the `winit`/`wgpu` host and a fixed-advance text measurer.
- Rendering is animated with `winit`'s `ControlFlow::Poll` and
  `Window::request_redraw`; no timer thread is used.
- The surface format prefers a non-sRGB format so shader output matches the
  Canvas backend; if only sRGB is offered, that is used as a fallback.
- The backend is window-agnostic: it receives a surface texture view via
  `WgpuBackend::begin_frame_with_view` and the demo calls `present()`. The same
  backend runs headlessly in `crates/draw_backend_wgpu/tests/render.rs`.
