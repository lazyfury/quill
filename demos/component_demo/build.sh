#!/usr/bin/env bash
# Builds the component demo into demos/component_demo/dist.
set -euo pipefail
cd "$(dirname "$0")/../.."

TARGET_DIR="target/wasm32-unknown-unknown/release"
OUT_DIR="demos/component_demo/dist"

cargo build -p component_demo --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript --out-dir "$OUT_DIR" "$TARGET_DIR/component_demo.wasm"

echo
echo "Built $OUT_DIR/component_demo.js + component_demo_bg.wasm"
echo "Serve with:"
echo "  python3 -m http.server 8080 --directory demos/component_demo"
echo "Then open http://localhost:8080/"
