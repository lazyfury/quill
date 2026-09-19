# quill macOS demo

Native macOS app using `draw_backend_coregraphics` (Core Graphics + Core Text via
the `objc2` bindings). Same `Scene`/`UI`/`DrawList` as the web demos, no browser,
no GPU.

## Modes

```bash
cargo run -p macos_demo -- --offscreen /tmp/quill.png  # one frame -> PNG, exits
cargo run -p macos_demo                                # AppKit window
cargo run -p macos_demo -- --selftest                  # render a live window frame,
                                                       # assert its pixel buffer, exit
```

- **Offscreen** renders into a `CGBitmapContext` at DPR 2 and writes a PNG.
- **Window** shows an `NSWindow` + `NSImageView`, re-rendered ~60 Hz from an
  `NSTimer`; the `DrawList` is produced by the shared `Demo`.
- **Self-test** exercises the real window/timer/DPR/`NSImage` path and checks the
  backend's pixel buffer for expected colors, then exits (no screen capture).

## What it shows

- a rotated `Node2D` with a child (scene transform propagation),
- a `Panel { Label, Button, status }` component UI,
- Core Graphics shape/transform/clip mapping,
- Core Text text rendering.

## Verification

`cargo test -p draw_backend_coregraphics` asserts colors/transform/clip directly
in the backend pixel buffer. `--selftest` does the same for the live window frame.
No screenshots or screen recordings are used (see `docs/testing.md`).
