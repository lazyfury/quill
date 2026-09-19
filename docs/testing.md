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

## What needs a browser

- `draw_backend_canvas` + `draw_wasm` (Canvas 2D) — a browser.
- `demos/web_demo`, `demos/component_demo` — a browser (functionality is also
  covered by native `draw_ui` tests).

`draw_backend_wgpu` needs no browser: it renders offscreen and reads pixels back,
so it runs under plain `cargo test`. Its windowed demo `demos/wgpu_demo` opens a
real window and cannot be verified without a display; it is compiled by
`cargo check` and run manually, and the render path it uses is the same one
covered by the readback tests.
