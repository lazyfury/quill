# Components

`draw_ui` provides a small UI layer on top of the `draw_scene` tree. A [`Ui`]
owns a `SceneTree` of `Control` nodes plus per-control layout (`ControlData`) and
behavior (`Widget`).

## Create components

```rust
use draw_ui::Ui;
use draw_core::{Size, Viewport};

let mut ui = Ui::new();               // root control fills the viewport
let panel = ui.add_panel(ui.root());
let vbox = ui.add_vbox(panel);
let label = ui.add_label(vbox, "Hello");
let button = ui.add_button(vbox, "Click me");

ui.layout(Viewport::new(Size::new(800.0, 600.0)));
```

Available widgets: `Panel`, `VBox`, `HBox`, `Label`, `Button`.

## Compose

Widgets are just controls added under a parent. Containers (`VBox`/`HBox`)
arrange their direct children in order; everything else uses anchors/offsets.

## Layout

Controls use the Godot-style anchor model:

```
left = parent.left + parent.width  * anchor.left  + offset.left
right = parent.left + parent.width * anchor.right + offset.right
```

```rust
use draw_core::Edges;

// fill the parent
ui.set_anchors(panel, Edges::new(0.0, 0.0, 1.0, 1.0));
ui.set_offsets(panel, Edges::ZERO);

// pin to the top-right, 320x300
ui.set_anchors(panel, Edges::new(1.0, 0.0, 1.0, 0.0));
ui.set_offsets(panel, Edges::new(-360.0, 40.0, -40.0, 340.0));

ui.set_min_size(button, Size::new(140.0, 44.0));
```

`VBox`/`HBox` use `BoxLayout { separation, padding }` and give each child the
content width/height from the child's minimum size. Call `ui.layout(viewport)`
after any change and on resize.

## Respond to input

```rust
use draw_core::{InputEvent, PointerButton, EventResult};

// Route a backend-neutral event; returns whether it was consumed.
let result: EventResult = ui.handle_input(&InputEvent::PointerDown {
    position: draw_core::Vec2::new(100.0, 100.0),
    button: PointerButton::Left,
});

// React to activation:
ui.set_on_click(button, || { /* mutate app state */ });

// Or poll state:
let count = ui.click_count(button);
```

Hit testing returns the topmost control under a point, honoring `MouseFilter`
(`Stop`/`Pass`/`Ignore`) and visibility. Keyboard (`Enter`/`Space`) activates the
focused button. MVP does target dispatch; capture/bubble is a future extension.

## Request redraw

The UI is immediate-mode over a persistent tree: mutate via `Ui`, call
`ui.layout(viewport)`, then `ui.paint(&mut ctx)` into a `PaintContext`. A new
`DrawList` is produced each frame from the current control state — no retained
GPU state.

```rust
let mut ctx = draw_render::PaintContext::new();
ui.paint(&mut ctx);
let list = ctx.into_draw_list();
```

## Extend with a custom component

Add a `Widget` variant (or a control subclass in your app) and emit its commands
in `Ui::paint`; layout is already handled by `ControlData`. Keep behavior driven
only by core state so it stays headless-testable.

## Full example

See `demos/web_demo` for a `Panel { Label, Button }` wired to a click counter,
input via `draw_wasm`, and responsive layout.
