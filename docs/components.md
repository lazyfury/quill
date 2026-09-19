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

Available components: `Panel`, `Flex`, `VBox`, `HBox`, `Grid`, `Label`,
`Button`. `ControlRef` is an owned, `Copy` handle; `ref.id()` gives the
underlying `NodeId`.

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
ui.set_anchors(panel.id(), Edges::new(0.0, 0.0, 1.0, 1.0));
ui.set_offsets(panel.id(), Edges::ZERO);

// pin to the top-right, 320x300
ui.set_anchors(panel.id(), Edges::new(1.0, 0.0, 1.0, 0.0));
ui.set_offsets(panel.id(), Edges::new(-360.0, 40.0, -40.0, 340.0));
```

### Flex

`VBox`/`HBox` are convenience column/row flex containers. For full control use
`Flex` (or `ui.add_flex`):

```rust
use draw_ui::{Align, Flex, Justify};

let row = ui.add(
    panel.id(),
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

Per-child sizing is set on the control and read by its parent:

```rust
use draw_ui::{LayoutStyle, SizeBasis};

// grow to fill leftover main-axis space ("拉伸/撑开")
ui.set_flex_grow(button.id(), 1.0);
// or replace the whole participation record
ui.set_layout_style(button.id(), LayoutStyle::new().grow(1.0).basis(SizeBasis::Px(120.0)));
// paint/placement order within the parent (lower first)
ui.set_layout_style(button.id(), LayoutStyle::new().order(-1));
```

`grow` absorbs leftover space, `shrink` resists overflow (weighted by basis),
`basis` chooses `Auto`/`Px`/`Percent`, and `LayoutStyle::align_self` overrides
the container's cross-axis alignment for one child.

### Grid

```rust
use draw_ui::{Align, AlignContent, Grid, GridPlacement, Track};

let grid = ui.add(
    panel.id(),
    Grid::new(vec![Track::Px(120.0), Track::Fr(1.0), Track::Fr(2.0)])
        .rows(vec![Track::Auto, Track::Px(40.0)])
        .align_items(Align::Center)    // vertical within the cell
        .justify_items(Align::Stretch) // horizontal within the cell
        .align_content(AlignContent::Stretch) // distribute rows
        .gap(8.0),
);

// explicit cell placement (optional; otherwise auto-flow row-major)
ui.set_layout_style(cell.id(), LayoutStyle::new().grid(GridPlacement::new(1, 0).column_span(2)));
```

Auto tracks grow to fit items that span multiple tracks. Explicit placement
skips occupied cells; auto-flow then fills the remaining cells row-major.
`LayoutStyle::order` also controls auto-placement order.

### Text wrapping

Labels wrap automatically to their resolved width. Wrapping breaks on explicit
newlines and spaces, and between East-Asian wide characters; overlong words are
hard-broken. A stretched label reports the taller `preferred` height it needs
for its wrapped lines.

`Label` exposes overflow options:

```rust
ui.add(
    panel.id(),
    Label::new("A long paragraph ...")
        .max_lines(2)
        .ellipsis(true),   // clip to 2 lines with “…”; `wrap(false)` disables soft wrap
);
```

`Button` accepts the same `.wrap(bool)` / `.max_lines(n)` / `.ellipsis(bool)`
builders (wrapping is off by default); a wrapped button grows its height and
centers each line.

Measurement is pluggable via `TextMeasurer`, so layout stays deterministic and
backend-neutral while the host supplies real metrics:

```rust
use std::rc::Rc;
use draw_ui::{FixedWidthTextMeasurer, TextMeasurer};

// e.g. match a fixed-width bitmap-font backend
ui.set_text_measurer(Rc::new(FixedWidthTextMeasurer::default()));
```

The default is `ApproxTextMeasurer` (proportional estimate). Injecting a
measurer invalidates layout.

### Redraw / layout caching

`Ui` caches the last resolved `Viewport` and skips measure/arrange unless it is
invalidated. Inserting controls, changing anchors/offsets/layout style, changing
text, `tree_mut()`, swapping the measurer, or a new viewport size all mark it
dirty. A change only dirties that node and its ancestors, so clean sibling
subtrees whose resolved rects are unchanged are skipped (partial relayout). Call
`ui.layout(viewport)` after changes; `Ui::layout_count()` and
`Ui::last_arranged_nodes()` report the work done, and `Ui::invalidate_layout()`
forces a full pass.

Within a pass, repeated measurements of the same control are memoized, and
paint reuses a per-control cache of wrapped/clipped lines until its text, font,
width, `TextOptions`, or the measurer changes.

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
