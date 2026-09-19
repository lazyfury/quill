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

- a filled + stroked rectangle,
- a filled circle,
- a rotated `Node2D` with a circular child (transform propagation),
- centered text,
- a resize-responsive panel and background.

The page sets `data-quill-status="painted"` after the first frame if real pixels
were detected; useful for headless smoke checks.
