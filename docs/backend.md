# Backends

`RenderBackend` consumes a `DrawList` and produces output. Implementations:

- `draw_backend_recording` — headless, used for deterministic tests.
- `draw_backend_canvas` — HTML Canvas 2D. Uses browser APIs; kept out of core.

## Adding a backend

1. Depend on `draw_render` only.
2. Implement the `RenderBackend` trait.
3. Never pull core/scene/UI code in the other direction.

Details land in Stages 4-5.
