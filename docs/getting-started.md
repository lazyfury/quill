# Getting Started

## Prerequisites

- Rust (stable). Workspace MSRV: 1.75.
- For the web demo: `wasm32-unknown-unknown` target and `wasm-bindgen-cli` 0.2.128.

## Build & test (native, no browser)

```bash
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
```

## Run the web demo

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.128   # match the wasm-bindgen dep

./demos/web_demo/build.sh
python3 -m http.server 8080 --directory demos/web_demo
# open http://localhost:8080/
```

The demo shows a rectangle, a circle, a rotated `Node2D` with a child, text, and a
viewport-responsive panel (resize the window).

## Create your first scene

```rust
use draw_core::Vec2;
use draw_scene::{SceneTree, Visual};
use draw_core::{Color, Size};

let mut tree = SceneTree::new();
let root = tree.root();

let node = tree.add_node2d(root, "Hero");
tree.set_position(node, Vec2::new(100.0, 80.0));
tree.set_rotation(node, 0.5);
tree.set_visual(node, Visual::Rect { size: Size::new(64.0, 64.0), color: Color::RED });

tree.update(); // derive world transforms / visibility
```

## Paint it to a `DrawList`

```rust
use draw_render::PaintContext;

let mut ctx = PaintContext::new();
tree.paint(&mut ctx);
let list = ctx.into_draw_list();
```

Then submit `list` to any `RenderBackend` (recording, Canvas 2D, ...). See
`docs/backend.md`.

## Create your first control

`Control` lands in Stage 6.
