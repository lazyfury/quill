# Components

UI is built from **components** on one `draw_scene::SceneTree`. A component is a
value that builds exactly one primary control node; attach it with
`SceneTree::add_child` and nest with `.child()`.

All UI runtime state lives on the tree (control data, widget, decorators, click
callback, GUI state, layout cache, text measurer). `draw_ui` is a set of free
functions over `&SceneTree` / `&mut SceneTree` — there is no `Ui` object. The
theme is a plain value passed to component constructors; it is never stored on
the tree.

`draw_components` is the widget API. Its `base` module owns the host-facing
`Component` trait and the unstyled primitives (`Flex`, `Panel`, `Label`, `Grid`,
`VBox`, `HBox`, `Column`, `Row`, plus the low-level `base::Button`); the crate
root adds the themed library (`Text`, `Card`, `Button`, `Checkbox`, `Switch`,
`ResizeHandle`, …) and the `Router` view switcher. Input routing lives in
`draw_ui` alongside layout and paint.

## Create & compose components

Attach primitives with `add_child`; compose a subtree with `.child(..)`:

```rust
use draw_components::base::Button;
use draw_components::{Component, Flex, Label, Panel, VBox};
use draw_scene::SceneTree;

let mut tree = SceneTree::new();

let root = tree.add_child(tree.root(), Flex::column().gap(8.0));
let panel = tree.add_child(root, Panel::new());
let vbox = tree.add_child(panel, VBox::new().separation(12.0));
let label = tree.add_child(vbox, Label::new("Hello"));

let button = tree.add_child(
    vbox,
    Button::new("Click me").on_click(|| { /* mutate app state */ }),
);

// Equivalent, but the whole subtree is one value:
tree.add_child(
    root,
    Panel::new()
        .child(Label::new("Settings"))
        .child(Button::new("Save")),
);

draw_ui::layout(&mut tree, viewport);
draw_ui::paint(&tree, &mut ctx);
```

`add_child` returns the created `NodeId`. Every node-mutating setter is a
`Component` method (`grow`, `min_size`, `anchors`, `offsets`, `order`,
`background`, `surface`, `foreground`, `on_click`, `mouse_filter`, `child`), so
they chain after any component.

## Layout

Layout is a two-pass traversal. First **measure** computes each control's
intrinsic size (`ContentSize { min, preferred }`, with text already wrapped to
the offered width); then **arrange** assigns absolute rectangles top-down.
Containers own their children's rectangles.

### Anchors (default)

Non-container children use the Godot-style anchor model:

```
left = parent.left + parent.width  * anchor.left  + offset.left
right = parent.left + parent.width * anchor.right + offset.right
```

```rust
use draw_core::Edges;

// fill the parent
tree.add_child(
    root,
    Panel::new()
        .anchors(Edges::new(0.0, 0.0, 1.0, 1.0))
        .offsets(Edges::ZERO),
);

// pin to the top-right, 320x300
tree.add_child(
    root,
    Panel::new()
        .anchors(Edges::new(1.0, 0.0, 1.0, 0.0))
        .offsets(Edges::new(-360.0, 40.0, -40.0, 340.0)),
);
```

### Flex

`VBox`/`HBox` are convenience column/row flex containers. For full control use
`Flex`:

```rust
use draw_components::Flex;
use draw_ui::{Align, Justify};

let row = tree.add_child(
    panel,
    Flex::row()
        .justify(Justify::SpaceBetween)
        .align(Align::Center)
        .gap(12.0),
);
```

- `justify`: main-axis distribution (`Start`/`Center`/`End`/`SpaceBetween`/
  `SpaceAround`/`SpaceEvenly`).
- `align`: cross-axis alignment of items (`Start`/`Center`/`End`/`Stretch`,
  default `Stretch`).
- `align_content`: distribution of wrapped lines along the cross axis
  (`Start`/`Center`/`End`/`SpaceBetween`/`SpaceAround`/`SpaceEvenly`/`Stretch`).
- `gap` (a.k.a. `separation`), `cross_gap` (wrapped-line gap), `padding`,
  `wrap` for multi-line rows.

Per-child sizing is a `Component` modifier:

```rust
use draw_ui::SizeBasis;

tree.add_child(
    row,
    Panel::new()
        .grow(1.0)                       // absorb leftover main-axis space
        .basis(SizeBasis::Px(120.0))     // Auto / Px / Percent
        .shrink(0.0)                     // resist overflow
        .order(-1),                      // paint/placement order within the parent
);
```

`grow` absorbs leftover space, `shrink` resists overflow (weighted by basis),
and `LayoutStyle::align_self` overrides the container's cross-axis alignment for
one child.

### Grid

```rust
use draw_components::Grid;
use draw_ui::{Align, AlignContent, GridPlacement, Track};

let grid = tree.add_child(
    panel,
    Grid::new(vec![Track::Px(120.0), Track::Fr(1.0), Track::Fr(2.0)])
        .rows(vec![Track::Auto, Track::Px(40.0)])
        .align_items(Align::Center)    // vertical within the cell
        .justify_items(Align::Stretch) // horizontal within the cell
        .align_content(AlignContent::Stretch)
        .gap(8.0),
);

// explicit cell placement via the released context id
draw_components::update_control(&mut tree, cell, |data| {
    data.layout.grid = GridPlacement::new(1, 0).column_span(2);
});
```

Auto tracks grow to fit items that span multiple tracks. Explicit placement
skips occupied cells; auto-flow then fills the remaining cells row-major.
`LayoutStyle::order` also controls auto-placement order.

### Text wrapping

Labels wrap automatically to their resolved width. Wrapping breaks on explicit
newlines and spaces, and between East-Asian wide characters; overlong words are
hard-broken. A stretched label reports the taller `preferred` height it needs
for its wrapped lines.

`Label` (and themed `Text`) expose overflow options:

```rust
tree.add_child(
    panel,
    Label::new("A long paragraph ...")
        .max_lines(2)
        .ellipsis(true),   // clip to 2 lines with “…”; `.wrap(false)` disables soft wrap
);
```

Measurement is pluggable via `TextMeasurer`, so layout stays deterministic and
backend-neutral while the host supplies real metrics:

```rust
use std::rc::Rc;
use draw_ui::FixedWidthTextMeasurer;

draw_ui::set_text_measurer(&mut tree, Rc::new(FixedWidthTextMeasurer::default()));
```

The default is `ApproxTextMeasurer` (proportional estimate). Injecting a
measurer invalidates layout. The measurer lives on the tree root; the theme does
not.

### Redraw / layout caching

The layout cache stored on the tree root caches the last resolved `ViewportSize`
and skips measure/arrange unless invalidated. Inserting controls, changing
anchors/offsets/layout style, changing text, swapping the measurer, or a new
viewport size all mark it dirty. A change only dirties that node and its
ancestors, so clean sibling subtrees whose resolved rects are unchanged are
skipped (partial relayout). Call `draw_ui::layout(&mut tree, viewport)` after
changes; `draw_ui::layout_count(&tree)` and `draw_ui::last_arranged_nodes(&tree)`
report the work done, and `draw_ui::invalidate_layout(&mut tree)` forces a full
pass.

Within a pass, repeated measurements of the same control are memoized, and paint
reuses a per-control cache of wrapped/clipped lines until its text, font, width,
`TextOptions`, or the measurer changes.

## Respond to input

```rust
use draw_core::{EventResult, InputEvent, PointerButton};

let result: EventResult = draw_ui::handle_input(
    &mut tree,
    &InputEvent::PointerDown {
        position: draw_core::Vec2::new(100.0, 100.0),
        button: PointerButton::Left,
    },
);

draw_components::set_on_click(&mut tree, button, || { /* ... */ }); // or Button::on_click builder
let count = draw_components::click_count(&tree, button);
```

Hit testing returns the topmost control under a point, honoring `MouseFilter`
(`Stop`/`Pass`/`Ignore`) and visibility. Keyboard (`Enter`/`Space`) activates the
focused button. MVP does target dispatch; capture/bubble is a future extension.

### Drag / resize

A node can own a pointer drag with `Component::on_drag` (or
`draw_components::set_on_drag`). While held, the node captures the pointer: every
`PointerMove` is routed to it (even outside its rect) as a delta in logical
pixels, and `PointerUp` releases it. The callback receives a `DragPhase`
(`Start`/`Move`/`End`) plus the delta, so a component can react to the start and
end of a drag from inside itself.

```rust
tree.add_child(
    split,
    draw_components::ResizeHandle::vertical(theme)
        .target(sidebar)                 // pane whose flex basis changes
        .width(width.clone())            // Rc<Cell<f32>> current size
        .min(140.0)
        .max(400.0),
);
```

`ResizeHandle` looks like a `Divider` (1px line) but its node is a wider gutter
(`size`, default 6px) that can be grabbed. It sets the target's
`LayoutStyle.basis` on drag; the surrounding `Flex` re-adapts the other panes.
Fixed panes/gutters should use `shrink(0.0)`.

### Cursor feedback

`ControlData.cursor` carries a backend-neutral `draw_core::Cursor`
(`Default`/`Pointer`/`Text`/`ColResize`/`RowResize`/`Grab`/`Grabbing`). Set it
with `Component::cursor(..)`; `ResizeHandle` sets `ColResize`/`RowResize` itself.
For a cursor that depends on the component's own state, use
`Component::dynamic_cursor(|| ..)` (a closure evaluated while the control is
hovered): `ResizeHandle` returns `Grabbing` while dragging and the resize cursor
otherwise.

`draw_ui::hovered_cursor(&tree)` returns the hovered control's cursor (nearest
ancestor that set one, dynamic provider first), falling back to `Pointer` for
anything with a click/drag callback. The cursor *value* is backend-neutral; only
the final application is platform code: hosts map it onto winit `CursorIcon`
or the CSS `cursor` property (`draw_wasm::App::cursor`).

## Switch views (Router)

`draw_components::Router` shows exactly one of several child views in a
container and hides the rest. All views stay mounted (their state survives a
switch), and `draw_ui` skips hidden controls in measure / arrange / paint /
hit-test, so only the active view occupies the pane.

```rust
use std::cell::Cell;
use std::rc::Rc;
use draw_components::{Button, Panel, Router};
use draw_core::Color;

let route = Rc::new(Cell::new(0));
let pane = tree.add_child(root, Panel::new().color(Color::TRANSPARENT).flat());
let notes = tree.add_child(pane, /* a Column built as the note view */);
let settings = tree.add_child(pane, /* a Column built as the settings view */);

let mut router = Router::with_route(pane, route.clone());
router.add_node(notes);
router.add_node(settings);
router.sync(&mut tree);            // apply route 0

// Click callbacks only write the shared cell:
Button::ghost("Settings", theme).on_click({ let r = route.clone(); move || r.set(1) });

// Once per frame, apply the route (hides the old view, marks it for relayout):
router.sync(&mut tree);
```

`Router::add` builds a component under the router; `add_node` tracks an
already-built node (handy when the view's internals are captured as it is
composed, as `demo_app` does). `go(tree, i)` switches and applies immediately;
`add_named` + `go_name(tree, "settings")` route by name. The host applies the
route once per frame (typically in its `update`).

## Request redraw

The UI is immediate-mode over a persistent tree. Mutate state, call
`draw_ui::layout(&mut tree, viewport)`, then paint a fresh `DrawList` each frame:

```rust
let mut ctx = draw_render::PaintContext::new();
draw_ui::paint(&tree, &mut ctx);
let list = ctx.into_draw_list();
```

## Extend with a custom component

Implement `draw_components::Component` for your own builder and attach it with
`add_child`:

```rust
use draw_components::{Component, Flex, Spec};
use draw_core::{Color, NodeId};
use draw_scene::SceneTree;
use draw_ui::Widget;

struct Badge {
    spec: Spec,
    text: String,
    color: Color,
}

impl Component for Badge {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Badge"
    }

    fn widget(&self) -> Widget {
        Widget::Label {
            text: self.text.clone(),
            font_size: 12.0,
            color: self.color,
            options: draw_ui::TextOptions::no_wrap(),
        }
    }
}

draw_components::impl_scene_child!(Badge);
```

The default `build` creates the control, installs `widget()`, and applies the
spec (layout, background/foreground, click callback, `.child()` list). Override
`prepare(&mut self)` to compute decorators/children from field values, and
`build` for fully custom composites.

Keep behavior driven only by core state so components stay headless-testable.

## Full example

See `examples/demo_app` for the complete recommended pattern (composition,
layout, `on_click`, state -> UI, WASM attach) built from the themed component
library.
