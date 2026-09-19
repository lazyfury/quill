#!/usr/bin/env bash
# Builds the wasm demo into demos/web_demo/dist.
set -euo pipefail
cd "$(dirname "$0")/../.."

TARGET_DIR="target/wasm32-unknown-unknown/release"
OUT_DIR="demos/web_demo/dist"

cargo build -p web_demo --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript --out-dir "$OUT_DIR" "$TARGET_DIR/web_demo.wasm"

echo
echo "Built $OUT_DIR/web_demo.js + web_demo_bg.wasm"
echo "Serve with:"
echo "  python3 -m http.server 8080 --directory demos/web_demo"
echo "Then open http://localhost:8080/"
