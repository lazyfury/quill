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

```rust
use draw_core::{Size, Viewport};
use draw_ui::{Ui, Panel, VBox, Label, Button};

let mut ui = Ui::new();

let panel = ui.add(ui.root(), Panel::new());
let vbox = ui.add(panel.id(), VBox::new());
ui.add(vbox.id(), Label::new("Hello"));

let button = ui.add(
    vbox.id(),
    Button::new("Click me").on_click(|| println!("clicked!")),
);

ui.layout(Viewport::new(Size::new(800.0, 600.0)));

// Pointer/keyboard input (backend-neutral):
ui.handle_input(&draw_core::InputEvent::PointerDown {
    position: draw_core::Vec2::new(100.0, 100.0),
    button: draw_core::PointerButton::Left,
});

// Paint into a DrawList:
let mut ctx = draw_render::PaintContext::new();
ui.paint(&mut ctx);
```

See `docs/components.md` for anchors, containers, events and custom components,
and `demos/component_demo` for a runnable browser example.

## Inspect performance

Measure the pipeline phases, aggregate them, and show a debug panel:

```rust
use draw_profile::{inspect, FrameCounters, FrameStats, Profiler, StageTimes};
use draw_debug_ui::DebugOverlay;

let mut profiler = Profiler::new();
let mut overlay = DebugOverlay::new();

// per frame (t0..t4 are Instant::now() samples around update/layout/paint/render)
let stats = FrameStats {
    index: profiler.next_index(),
    frame_ms: 12.0,
    stages: StageTimes::new(1.0, 0.5, 2.0, 8.5),
    counters: FrameCounters::new(scene_nodes, controls, list.len(), 1),
};
profiler.record(stats);

let report = inspect(&list, &stats);

overlay.update(&profiler, &report, viewport);
overlay.paint(&mut ctx); // painted after your own UI
```

`draw_profile` never reads the clock itself; the host feeds in milliseconds, so
metrics are deterministic and testable. See `docs/debug.md` for the full guide
(phase wiring, thresholds, finding codes, overlay styling).
