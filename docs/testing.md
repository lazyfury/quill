# Testing

## Layers

- Math tests — `draw_core`.
- SceneTree / transform / visibility tests — `draw_scene`.
- DrawList / golden tests — `draw_render` + `draw_scene::paint`.
- RecordingBackend assertions — `draw_backend_recording` (`CommandAsserts`,
  `tests/pipeline.rs`).
- Layout / hit-test / input tests — `draw_ui` unit tests.
- Native backend pixel tests — `draw_backend_coregraphics/tests/render.rs`
  (asserts the backend's own pixel buffer).

Core behavior must be testable with native `cargo test`, without a browser.
Only the Canvas backend and WASM glue need a browser.

## Hard rule: no screenshot / screen-recording testing

Never verify rendering with `screencapture`, browser screenshots, screen
recording, or any OS/window capture. Verify programmatically instead:

- **Backend pixel buffers** — e.g. `CoreGraphicsBackend::pixels()`; assert RGBA/
  BGRA values at known coordinates (see `tests/render.rs`).
- **DrawList command sequences** — `CommandAsserts` and golden comparisons.
- **DOM state markers** — the web demos expose `data-quill-*` attributes that
  headless checks read from `--dump-dom` (no image capture).
- **In-app self-tests** — `macos_demo --selftest` renders a live window frame and
  asserts the pixel buffer, then exits.

If a claim cannot be verified without a screenshot, say so explicitly rather than
capturing one.

## Golden / snapshot tests

`DrawList` is deterministic. `draw_scene`'s `scene_to_draw_list_is_deterministic`
and `draw_render`'s `drawing_is_deterministic` compare exact command sequences.
Extend by asserting the `Vec<DrawCommand>` directly.

## What needs a browser / window

- `draw_backend_canvas` + `draw_wasm` (Canvas 2D) — a browser.
- `demos/web_demo`, `demos/component_demo` — a browser (functionality is also
  covered by native `draw_ui` tests).
- `demos/macos_demo` window mode — a GUI session (pixel verification is
  available via `--selftest` and the offscreen PNG; no screen capture is used).
