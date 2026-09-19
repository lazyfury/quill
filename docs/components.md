# Components

`draw_ui` provides a reusable component API on top of the `draw_scene` tree. A
[`Ui`] owns a `SceneTree` of `Control` nodes plus per-control layout and behavior.

## Create & compose components

Mount small builder components with `Ui::add`:

```rust
use draw_ui::{Ui, Panel, VBox, Label, Button};

let mut ui = Ui::new();

let panel = ui.add(ui.root(), Panel::new());
let vbox = ui.add(panel.id(), VBox::new().separation(12.0));
let label = ui.add(vbox.id(), Label::new("Hello"));

let button = ui.add(
    vbox.id(),
    Button::new("Click me").on_click(|| { /* mutate app state */ }),
);
```

Available components: `Panel`, `VBox`, `HBox`, `Label`, `Button`. `ControlRef`
is an owned, `Copy` handle; `ref.id()` gives the underlying `NodeId`.

## Layout

Controls use the Godot-style anchor model:

```
left = parent.left + parent.width  * anchor.left  + offset.left
right = parent.left + parent.width * anchor.right + offset.right
```

```rust
use draw_core::Edges;

// fill the parent
ui.set_anchors(panel.id(), Edges::new(0.0, 0.0, 1.0, 1.0));
ui.set_offsets(panel.id(), Edges::ZERO);

// pin to the top-right, 320x300
ui.set_anchors(panel.id(), Edges::new(1.0, 0.0, 1.0, 0.0));
ui.set_offsets(panel.id(), Edges::new(-360.0, 40.0, -40.0, 340.0));

ui.set_min_size(button.id(), draw_core::Size::new(140.0, 44.0));
```

`VBox`/`HBox` arrange their direct children with a separation and padding from
`BoxLayout`. Call `ui.layout(viewport)` after changes and on resize.

## Respond to input

```rust
use draw_core::{EventResult, InputEvent, PointerButton};

let result: EventResult = ui.handle_input(&InputEvent::PointerDown {
    position: draw_core::Vec2::new(100.0, 100.0),
    button: PointerButton::Left,
});

ui.set_on_click(button.id(), || { /* ... */ });  // or Button::on_click builder
let count = ui.click_count(button.id());
```

Hit testing returns the topmost control under a point, honoring `MouseFilter`
(`Stop`/`Pass`/`Ignore`) and visibility. Keyboard (`Enter`/`Space`) activates the
focused button. MVP does target dispatch; capture/bubble is a future extension.

## Request redraw

The UI is immediate-mode over a persistent tree. Mutate state, call
`ui.layout(viewport)`, then paint a fresh `DrawList` each frame:

```rust
let mut ctx = draw_render::PaintContext::new();
ui.paint(&mut ctx);
let list = ctx.into_draw_list();
```

## Extend with a custom component

Implement `Component` for your own builder and mount it with `Ui::add`:

```rust
use draw_ui::{Component, ControlRef};
use draw_core::NodeId;

struct Badge { text: String }

impl Component for Badge {
    fn mount(self, ui: &mut draw_ui::Ui, parent: NodeId) -> ControlRef {
        let panel = ui.add(parent, draw_ui::Panel::new());
        ui.add(panel.id(), draw_ui::Label::new(self.text));
        panel
    }
}
```

Keep behavior driven only by core state so components stay headless-testable.

## Full example

See `demos/component_demo` for the complete recommended pattern (composition,
layout, `on_click`, state -> UI, WASM attach).
