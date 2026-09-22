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

- a component gallery built from `draw_components` on the shared
  `demo_app::DemoApp`: a sidebar of groups, a preview `Router` and live cards
  (`Text`, `Card`, `Badge`, `Button`, `Checkbox`, `Switch`, `List`, …),
- theme switching (the sidebar footer toggles light/dark),
- interactive previews (buttons, toggles, a resizable split, menus and dialogs),
- pointer input (hover/click) and viewport-responsive layout (resize the window).

## Headless self-test

The page sets `data-quill-status="painted"` after the first frame. It also
reports `data-quill-button="x,y"` and `data-quill-clicks="n"`. Open with
`?selftest=1` to dispatch real pointer events at the button and verify the click
counter increments (`data-quill-status="clicked"`).
