# Components

> For the app-level walkthrough (frame loop, hosting, conventions), start with
> `docs/ui-guide.md`; this page is the widget/layout/input reference.

UI is built from **components** on one `draw_scene::SceneTree`. A component is a
value that builds exactly one primary control node; mount a whole scene with
`.into_tree()` (or `SceneTree::add_child`) and nest with `.child()`.

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

Compose declaratively with `.child(..)` / `.children([..])`, then mount the
whole scene once with `.into_tree()`:

```rust
use draw_components::base::Button;
use draw_components::{Column, Label, Panel, Row};
use draw_scene::SceneTree;

let tree = Column::new()
    .gap(8.0)
    .child(Row::new().child(Label::new("Hello")))
    .child(
        Panel::new()
            .child(Label::new("Settings"))
            .child(Button::new("Save").on_click(|| { /* mutate app state */ })),
    )
    .into_tree();

draw_ui::layout(&mut tree, viewport);
draw_ui::paint(&tree, &mut ctx);
```

`tree.add_child(parent, child)` returns the new `NodeId` and stays for runtime
additions. Every node-mutating setter is a `Component` method (`grow`,
`min_size`, `anchors`, `offsets`, `order`, `background`, `surface`,
`foreground`, `on_click`, `mouse_filter`, `child`, `children`, `ref_`,
`with_ref`), so they chain after any component.

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
Fixed panes/gutters should use `shrink(0.0)`. By default the target is the pane
before the handle; add `.invert()` when it is on the far side (a right sidebar
resized from its left edge, so dragging left grows it).

One gutter resizes two panes: give the *left* pane a `basis` and let the right one
`grow(1.0)`, then point the handle at the left pane — dragging left shrinks it and
the right pane grows by exactly that much. The handle's `min` / `max` are
build-time constants, so it cannot clamp against the *window*: a host whose window
can shrink has to re-clamp the width itself on every `layout` (see
`examples/file_browser`, which keeps `viewport - PREVIEW_MIN - gutter` as the
ceiling) or the flexible pane disappears.

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

## Scroll & virtualized lists

Two pieces make scrolling possible in the core, and `List` builds on both.

**Clipping** is opt-in and lives on `ControlData.clip`: a control that clips
hands its own rectangle to its whole subtree (`draw_ui::set_clip`,
`Component::clip(true)`). Nested clips intersect, and a subtree whose
intersection is empty is skipped entirely — nothing painted, nothing
hit-testable. `Ui::layout` resolves the rectangle in the same pre-order pass
that writes the rectangles back, so it is a pure function of the geometry and
needs no dirty propagation of its own; paint emits one `save` + `clip_rect` per
clipped region and pops it with `restore`. A tree where nothing clips emits no
clip commands and pays nothing.

**Wheel routing**: `handle_input` hit-tests `InputEvent::Wheel { position,
delta }` and hands it to the nearest ancestor with a scroll callback
(`draw_components::set_on_scroll`, or `Component::on_scroll` — the wheel counterpart of
`on_click`/`on_drag`). It returns `Handled` only when a callback took it, so an
unclaimed wheel still reaches the host.

The *core* routes the wheel; a host still has to produce it. `examples/file_browser`
is the reference pump: `host::wheel_pixels` turns winit's `MouseScrollDelta` into
logical pixels (`y > 0` scrolls down) and feeds `InputEvent::Wheel`. The event's
sign convention is the one knob to flip if a platform reports the opposite, and
the function is a pure one-liner with tests. Neither `wgpu_demo` (winit
`WindowEvent::MouseWheel`) nor the Canvas runner (DOM `wheel`) has that
translation yet, so a list inside those demos does not scroll until it is added —
`handle_input` is the contract, the pump is the host's job.

`draw_components::List` mounts only the rows its viewport can show and recycles
them as it scrolls, so the node count, the layout work and the emitted commands
follow the viewport rather than the data:

```rust
use std::cell::Cell;
use std::rc::Rc;
use draw_components::{List, ListColumn};

let entries = /* your data */;
let count = Rc::new(Cell::new(entries.len()));
let list = List::new(theme, 28.0, move |index| vec![
    entries[index].name.clone(),
    entries[index].size.clone(),
])
.columns(vec![ListColumn::flexible(), ListColumn::fixed(96.0)])
.count(count.clone())
.on_activate(|index| println!("open {index}"));

let state = list.state();                    // take the handle before mounting
tree.add_child(pane, list.grow(1.0));

// Once per frame, after layout:
draw_ui::layout(&mut tree, viewport);
if state.sync(&mut tree) {                   // mounts/moves/re-binds the pool
    draw_ui::layout(&mut tree, viewport);    // a changed pool wants new rects
}
```

- `List::new(theme, row_height, source)` — `source` is a `RowSource`
  (`Rc<dyn Fn(usize) -> Vec<String>>`) called only for rows about to be shown,
  so the data never has to exist as widgets.
- `.columns(..)` — one `ListColumn` per cell: `flexible()` takes the leftover
  width, `fixed(w)` is fixed (and muted by default).
- `.count(Rc<Cell<usize>>)` — read on every sync, so a re-scan just sets the
  cell. `.invalidate()` re-reads the rows when the contents changed under the
  same count.
- `.selected(Rc<Cell<Option<usize>>>)` / `.on_activate(fn(usize))` — clicking a
  row selects it (the selection follows the *data* index, not the pool slot) and
  calls back.
- `ListState` — `sync`, `rows`, `visible_range`, `pool_size`, `offset`,
  `scroll_by(delta)`, `scroll_to(index)` (smallest scroll that brings `index`
  into view), `invalidate`, `selected`.
- The container clips and stops the wheel (`MouseFilter::Stop`), so the partial
  rows at its edges are cut off instead of bleeding over the pane above.

The pool is `ceil(viewport_height / row_height) + 1` rows: what fits, plus the
buffer that makes the next scroll step a pure offset change. Row count therefore
costs nothing per frame — `docs/benchmarking.md` has the measured shape
(107 controls and 72 commands per frame at 1 K, 10 K and 100 K rows, against a
naive list's 300 K controls and 200 K commands at 100 K).

`examples/file_browser` is the end-to-end example: a directory scanner feeding a
`List`, with the scan on a worker thread, keyboard navigation, and a headless
self-check that asserts the frame stays flat when the listing grows from 5 000
to 200 000 rows. Behind a `ResizeHandle`, its right pane holds a second view of
the selected file's first 64 KiB (4 096 rows of data, ~30 rows mounted) in two
modes — hex dump or text — which is the cheapest way to see that **one view can
hold several virtualized lists**: each needs its own `ListState::sync` in the
same frame step (`layout` → sync every list → `layout` again if any changed), and
a list whose container is hidden gets no rect, so `sync` returns early and it
owns no pool — the idle mode costs nothing.

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

## Menus

`Menu` is a floating surface of `MenuItem` rows; `Overlays::menu` drops it below
an anchor and handles Escape / click-outside. The host opens it in its frame
update, so click callbacks (which cannot borrow the host) only record a request.

```rust
use draw_components::{Menu, MenuItem};

// A menu title click records `Some(index)` in a shared cell...
Button::ghost("File", theme).on_click({ let r = request.clone(); move || r.set(Some(0)) });

// ...and `update` drains it into an overlay:
overlays.menu(title_node, move |tree, node| {
    tree.add_child(node, Menu::new(theme)
        .item(MenuItem::action("Undo", "Ctrl+Z", theme).on_click(undo).disabled(!can_undo))
        .separator()
        .item(MenuItem::new("Export PNG…", theme).on_click(export)));
});
```

`MenuItem` renders a left label and an optional right-aligned `shortcut`, takes a
`tone` (`destructive()` for delete), and `disabled(true)` dims it and drops the
click. `Menu::min_width` overrides the 200px default; the surface stretches each
row. A menu item click does not close the overlay by itself — the host closes it
when it drains the action (see `image_editor`).

The overlay consumes an outside click (that is how it dismisses), so a host whose
menu bar should switch menus in one click has to intercept the title hit *before*
delegating to `Overlays`: `image_editor` checks the pointer against its
menu-title nodes on `PointerDown`, closes the overlay, and lets the click reach
the tree (clicking the open title closes it; clicking another switches).

## Request redraw

The UI is immediate-mode over a persistent tree. Mutate state, call
`draw_ui::layout(&mut tree, viewport)`, then paint a fresh `DrawList` each frame:

```rust
let mut ctx = draw_render::PaintContext::new();
draw_ui::paint(&tree, &mut ctx);
let list = ctx.into_draw_list();
```

## Extend with a custom component

Implement `draw_components::Component` for your own builder and mount it with
`.into_tree()` / `add_child`:

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

## Component authoring style

`examples/demo_app` and the library follow one convention: a component is a pure
spec, its `NodeId` exists only after mount, and the tree is touched only at
mount time.

- **Compose** with `.child(..)` / `.children([..])` — deferred, no tree. Build
  leaf components as named locals first, then compose the root once at the end:

  ```rust
  let header = Row::new().child(app_icon(20.0, theme)).child(Text::subheading("Quill", theme));
  let rows = NOTES.iter().enumerate().map(|(i, note)| {
      let slots = handles.list_rows.clone();
      note_row_view(theme, note, i, &state.selected).with_ref(move |id| slots.borrow_mut().push(id))
  });

  let inner = Column::new().gap(space::XS).child(header).children(rows);
  ```

- **Composite components** come in two shapes:
  - root is an existing primitive → wrap it as `inner`, delegate
    `spec`/`widget`/`prepare`, and only override `name()`;
  - custom mount logic → own `spec: Spec`, hand-write `widget()`, override
    `build(self, tree, parent)`.

  ```rust
  struct Sidebar { inner: Column }
  impl Component for Sidebar {
      fn spec(&mut self) -> &mut Spec { self.inner.spec() }
      fn name(&self) -> &'static str { "Sidebar" }
      fn widget(&self) -> Widget { self.inner.widget() }
      fn prepare(&mut self) { self.inner.prepare(); }
  }
  ```

- **Reach a node after mount** with `NodeRef` + `ref_` / `with_ref`:

  ```rust
  let title = NodeRef::new();
  let header = Row::new().child(Text::subheading("Quill", theme).ref_(&title));
  let tree = Column::new().child(header).into_tree();
  if let Some(id) = title.get() { draw_components::set_text(&mut tree, id, "Inbox"); }
  ```

- **Add children in `prepare`** with `Spec::child` / `Spec::children` —
  `Component::child` needs `self` by value, which `prepare(&mut self)` lacks:

  ```rust
  self.spec.child(Label::new(text).font_size(size).color(color));
  ```

- **Mount** static scenes once with `.into_tree()` /
  `SceneTree::from_component(root)`. `tree.add_child` is the low-level primitive:
  use it only in `build` overrides, dynamic runtime (overlays, router views), and
  tools/tests/benches that capture ids.

- **State** is passed in as `Rc<Cell<_>>` (a `DemoState`-like struct) and changed
  through `on_click`/`on_drag` callbacks — never by reaching into the tree.

## Full example

See `examples/demo_app` for the complete recommended pattern (composition,
layout, `on_click`, state -> UI, WASM attach) built from the themed component
library.
