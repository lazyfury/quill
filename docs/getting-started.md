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

./examples/web_demo/build.sh
python3 -m http.server 8080 --directory examples/web_demo
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

Components compose into one node tree. Attach a component with
`SceneTree::add_child`; nest with `.child()`. The theme is a value passed to the
constructors, never stored on the tree.

```rust
use draw_components::base::Button;
use draw_components::{Component, Flex, Label, Panel, VBox};
use draw_core::{Size, ViewportSize};
use draw_scene::SceneTree;
use draw_theme::Theme;

let theme = Theme::dark();
let mut tree = SceneTree::new();

let root = tree.add_child(tree.root(), Flex::column());
let panel = tree.add_child(root, Panel::new());
let vbox = tree.add_child(panel, VBox::new());
tree.add_child(vbox, Label::new("Hello"));

let _button = tree.add_child(
    vbox,
    Button::new("Click me").on_click(|| println!("clicked!")),
);

// Or compose a subtree as a value before attaching it:
tree.add_child(
    root,
    Panel::new().child(Label::new("Composed")).child(Button::new("Save")),
);

draw_ui::layout(&mut tree, ViewportSize::new(Size::new(800.0, 600.0)));

// Pointer/keyboard input (backend-neutral):
draw_ui::route_input(
    &mut tree,
    &draw_core::InputEvent::PointerDown {
        position: draw_core::Vec2::new(100.0, 100.0),
        button: draw_core::PointerButton::Left,
    },
);

// Paint into a DrawList:
let mut ctx = draw_render::PaintContext::new();
draw_ui::paint(&tree, &mut ctx);
```

Themed components (`draw_components::Text`, `Card`, `Button`, `Checkbox`, …)
take the theme as their first argument: `Text::heading("Notes", theme)`,
`Card::new(theme)`. Start with `docs/ui-guide.md` for the app-level guide (frame
loop, hosting, conventions); `docs/components.md` is the widget/layout/input
reference. `examples/web_demo` is a runnable browser example.

## Debug component bounds

Outline every visible control in yellow with a `Name #id` label:

```rust
use draw_debug_ui::DebugOverlay;

let mut debug = DebugOverlay::new();

// per frame, after painting the UI into `ctx`:
debug.paint(&tree, &mut ctx);
```

`draw_ui::paint_debug` does the drawing; `DebugOverlay` just adds an
open/closed toggle. See `docs/debug.md`.

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
