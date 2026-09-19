# Testing

- Math tests — Stage 1.
- SceneTree / transform / visibility tests — Stage 2.
- DrawList / golden tests — Stage 3.
- RecordingBackend assertions — Stage 4.
- Layout / hit-test / input tests — Stage 6 (`draw_ui` unit tests).

Core behavior must be testable with native `cargo test`, without a browser.
Only the Canvas backend and WASM glue require browser verification.
