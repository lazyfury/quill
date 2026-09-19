# quill web demo

WASM + HTML Canvas 2D demo for the `quill` drawing core.

## Build & run

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.128   # must match the wasm-bindgen crate

./build.sh
python3 -m http.server 8080 --directory .
# open http://localhost:8080/
```

`build.sh` runs `cargo build -p web_demo --target wasm32-unknown-unknown
--release` and `wasm-bindgen --target web --out-dir dist`, producing
`dist/web_demo.js` + `dist/web_demo_bg.wasm`.

## What it shows

- a three-column, macOS-style notes app (sidebar / content list / detail) built
  from `draw_kit` themed components (`Text`, `Badge`, `Divider`, `Button`,
  `Checkbox`, `Switch`) on the shared `demo_app::DemoApp`,
- monochrome rounded-square placeholders for icons and images,
- a static image placeholder (monochrome rounded square) in the detail hero,
- selection state (click a note or nav row) reflected in the detail pane,
- pointer input (hover/click) and viewport-responsive layout (resize the window).

## Headless self-test

The page sets `data-quill-status="painted"` after the first frame. It also
reports `data-quill-button="x,y"` and `data-quill-clicks="n"`. Open with
`?selftest=1` to dispatch real pointer events at the button and verify the click
counter increments (`data-quill-status="clicked"`).
