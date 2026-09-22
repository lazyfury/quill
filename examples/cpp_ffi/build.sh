#!/usr/bin/env bash
# Builds the Rust FFI static library and the C++ OpenGL host.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

cargo build --release -p draw_ffi -p demoapp_ffi --manifest-path ../../Cargo.toml

cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel

echo
echo "built: $here/build/cpp_ffi"
echo "run:   $here/build/cpp_ffi --selfcheck"
echo "       $here/build/cpp_ffi --dump"
echo "       $here/build/cpp_ffi"
