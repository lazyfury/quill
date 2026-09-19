# Testing

## Layers

- Math tests — `draw_core`.
- SceneTree / transform / visibility tests — `draw_scene`.
- DrawList / golden tests — `draw_render` + `draw_scene::paint`.
- RecordingBackend assertions — `draw_backend_recording` (`CommandAsserts`,
  `tests/pipeline.rs`).
- wgpu pixel readback — `draw_backend_wgpu` renders to an offscreen texture and
  asserts on returned RGBA8 pixels (`tests/render.rs`); no window is created.
- Layout / hit-test / input tests — `draw_ui` unit tests.
- Profiler / inspector tests — `draw_profile` (ring buffer, summary math, every
  `FindingCode`, budget escalation). Pure data, no clock.
- Component debug tests — `draw_ui` asserts `paint_debug` emits one yellow
  `StrokeRect` + a `Name #id` `DrawText` per visible control; `draw_debug_ui`
  tests `DebugOverlay` (bounds, labels, open/closed, label options).
- Performance panel tests — `draw_debug_ui` asserts the computed `OverlayText`,
  that the labels mirror it, that the panel is pinned, and that paint emits the
  expected `DrawText`/`FillRect` commands (and nothing while closed).
- Benchmark harness tests — `draw_bench` asserts stats math, percentile
  interpolation, baseline text round-trips, verdict classification (including
  threshold boundaries) and the filter/runner behavior. These are pure data, no
  timing assertions.
- Benchmark scenario tests — `draw_bench_suite` asserts fixtures are
  deterministic (identical `DrawList`s), start clean, and hit-test to the
  expected control. They never assert on measured time.

Core behavior must be testable with native `cargo test`, without a browser.
Only the Canvas backend and WASM glue need a browser.

## Hard rule: no screenshot / screen-recording testing

Never verify rendering with `screencapture`, browser screenshots, screen
recording, or any OS/window capture. Verify programmatically instead:

- **Backend pixel/output assertions** — assert against what a backend produces
  (e.g. recorded command sequences via `CommandAsserts`).
- **DrawList command sequences** — `CommandAsserts` and golden comparisons.
- **DOM state markers** — the web demos expose `data-quill-*` attributes that
  headless checks read from `--dump-dom` (no image capture).

If a claim cannot be verified without a screenshot, say so explicitly rather than
capturing one.

## Golden / snapshot tests

`DrawList` is deterministic. `draw_scene`'s `scene_to_draw_list_is_deterministic`
and `draw_render`'s `drawing_is_deterministic` compare exact command sequences.
Extend by asserting the `Vec<DrawCommand>` directly.

## Benchmarks vs tests

Benchmarks live in `benches/` targets (`harness = false`) and are run with
`cargo bench`, never `cargo test`. They measure time and are machine-dependent,
so they are **not** part of the correctness suite and make no assertions on
measured time. What *is* tested is the harness itself (`draw_bench`) and the
determinism of every scenario fixture (`draw_bench_suite`). The `bench` profile is
pinned to `opt-level = 3`. See `docs/benchmarking.md`.

## What needs a browser

- `draw_backend_canvas` + `draw_wasm` (Canvas 2D) — a browser.
- `demos/web_demo`, `demos/component_demo` — a browser (functionality is also
  covered by native `draw_ui` tests).

`draw_backend_wgpu` needs no browser: it renders offscreen and reads pixels back,
so it runs under plain `cargo test`. Its windowed demo `demos/wgpu_demo` opens a
real window and cannot be verified without a display; it is compiled by
`cargo check` and run manually, and the render path it uses is the same one
covered by the readback tests.

The `demos/wgpu_demo` overlays are host-wired: the component bounds and the
performance panel text, layout and emitted commands are covered by
`draw_ui`/`draw_debug_ui` tests, and the numbers the panel displays come from
`draw_profile` (tested with injected durations). The windowed overlays themselves
are **not** screenshot-verified.
