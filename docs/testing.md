# Testing

- Math tests — Stage 1.
- SceneTree / transform / visibility tests — Stage 2.
- DrawList / golden tests — Stage 3.
- RecordingBackend assertions — Stage 4.
- Layout / hit-test tests — Stage 6.

Core behavior must be testable with native `cargo test`, without a browser.
Only the Canvas backend and WASM glue require browser verification.
