# Using the UI stack

A practical guide to building an app view with the workspace's UI layers, with
the conventions the demos follow. It is derived from `examples/demo_app`,
`examples/wgpu_demo`, `examples/deepseek_balance` and `examples/file_browser`.

- **In a hurry?** read [§1 the frame loop](#1-the-frame-loop) and the
  [agent cheat sheet](#10-agent-cheat-sheet--where-to-look).
- **Reference by API name?** `docs/components.md` (widgets, layout, input,
  extension) and `docs/design-system.md` (tokens, themed components).
- There is **no `quill` facade crate yet** (planned, Stage 25 Phase 9): depend on
  the individual `draw_*` crates.

## 1. The frame loop

Every app is the same four steps per frame. The tree is persistent; the frame is
immediate-mode over it.

```rust
// 1. state -> view: advance app state, then rebuild/refresh the tree
app.update(viewport, dt);
// 2. resolve geometry (and flush deferred tree changes)
draw_ui::layout(&mut tree, viewport);
tree.update();
// 3. emit this frame's backend-neutral draw list
let mut ctx = draw_render::PaintContext::new();
draw_ui::paint(&tree, &mut ctx);
let list = ctx.into_draw_list();
// 4. hand `list` to a backend (wgpu / canvas / recording)
```

- `draw_ui::layout(&mut tree, viewport)` is a two-pass measure + arrange. It is
  cached: only dirty subtrees relayout (`draw_ui::mark_dirty`, `invalidate_layout`).
- `tree.update()` runs deferred component work; `demo_app` calls it inside
  `DemoApp::layout` (`examples/demo_app/src/lib.rs`).
- `paint` walks the tree and emits into `PaintContext`; `into_draw_list()` is the
  complete, backend-neutral IR.

## 2. Build a view

A **component** is a value that builds exactly one control node. Compose with
`.child(..)` / `.children([..])`, then mount once with `.into_tree()`.

```rust
use draw_components::{Button, Card, Column, Divider, Row, Text};
use draw_theme::{space, Theme};

let theme = Theme::dark();
let tree = Column::new()
    .gap(space::MD)
    .child(Text::heading("Settings", theme))
    .child(
        Card::new(theme)
            .gap(space::SM)
            .child(Row::new().child(Text::small("Theme", theme)))
            .child(Divider::horizontal(theme))
            .child(Button::primary("Save", theme).on_click(|| { /* write a cell */ })),
    )
    .into_tree();
```

Choices that trip people up:

- `Column::new()` / `Row::new()` (and `VBox`/`HBox`) have **zero** default padding
  and gap. `Flex::column()` / `Flex::row()` use the `FlexStyle` default
  (**16px padding, 8px gap**). Pick deliberately; the demos use `Flex` for page
  layout and `Column`/`Row` for compact groups.
- Sizing is a component modifier: `grow`, `shrink`, `basis` (`SizeBasis::Px`),
  `min_size`, `order`; non-container children can use Godot-style
  `anchors(Edges)` + `offsets(Edges)`.
- Containers that should not eat clicks: `.mouse_filter(MouseFilter::Ignore)`.
- Text: `Text::{new, display, title, heading, subheading, small, caption}` then
  `.tone(Tone::Muted)` (or `.color(..)`), `.wrap(bool)`, `.max_lines(n)`,
  `.ellipsis(bool)`, `.size(TextSize)`.

Layout details (flex/grid/text wrapping): `docs/components.md` §Layout.

## 3. State, events and reaching nodes

State lives in `Rc<Cell<..>>` / `Rc<RefCell<..>>` captured by callbacks; the tree
is never the state store. Callbacks write cells, and the view drains them in
`update`.

```rust
let requested = Rc::new(Cell::new(false));
let button = Button::primary("Refresh", theme).on_click({
    let requested = requested.clone();
    move || requested.set(true)
});
// ... in `update`: if requested.replace(false) { /* do the work */ }
```

Input and interaction:

- Route events with `draw_ui::route_input(&mut tree, &event) -> EventResult`.
  Views handle their own shortcuts first (e.g. `R`/`Esc`) and return
  `EventResult::Handled`.
- `InputEvent::{PointerDown, PointerUp, PointerMove, PointerLeave, Wheel,
  KeyDown, KeyUp, TextInput}` (`draw_core`). Hosts map platform events to these.
- Cursor: `draw_ui::hovered_cursor(&tree) -> Cursor`; map it in the host
  (`deepseek_balance/src/host.rs`). Per-control provider: `.dynamic_cursor(..)`.
- Drag/resize: `.on_drag(..)` with `DragPhase::{Start, Move, End}` plus delta.
- Scroll: `.on_scroll(..)`; wheel routes to the nearest control with a scroll
  callback. Virtualized lists own this (`docs/components.md` §Scroll).

Reach a mounted node with `NodeRef` + `.ref_(&slot)` / `.with_ref(..)`, then
`draw_components::{set_text, set_on_click, set_on_drag, set_on_scroll}`:

```rust
let title = NodeRef::new();
let tree = Column::new().child(Text::heading("…", theme).ref_(&title)).into_tree();
if let Some(id) = title.get() {
    draw_components::set_text(&mut tree, id, "Inbox");
}
```

## 4. Change what's shown without rebuilding

- **Hide/show a page** with `SceneTree::set_visible` (+ `draw_ui::mark_dirty`):
  the hidden subtree leaves layout entirely. This is how `deepseek_balance` and
  `file_browser` do tabs.
- **Highlight the active tab** with `.dynamic_background(move |_state| ...)`,
  reading a shared cell — no tree rebuild on switch.
- **Swap whole views** with `Router`; **float** content (menus, dialogs, tips)
  with `Overlays`.

## 5. Advanced widgets

- `List` (virtualized): build columns, keep the returned `ListState`, and call
  `state.sync(&mut tree)` once per frame. Cost is flat in row count. See
  `examples/file_browser`.
- `ResizeHandle::vertical(theme).target(node_ref).width(cell).min(..).max(..)`
  resizes a pane's flex basis (`examples/file_browser`, `deepseek_balance`).
- `Overlays`: `Overlays::new(theme)` then `.confirm(..)`, `.popover(..)`,
  `.tips(..)`, `.message(..)`; each frame `update` → `layout(tree, viewport)` →
  `paint(ctx)` → `handle_input(event)`. `demo_app` wires all four.
- `Router`: named/added views with `go` / `go_name`; one route is laid out.

## 6. Hosting (winit + wgpu)

`examples/wgpu_demo` is the canonical host (`src/app.rs`). Checklist:

1. Create the `EventLoop`, set `ControlFlow::Wait` (repaint only on change).
2. In `ApplicationHandler::resumed`, create the `Window`.
3. `instance.create_surface(window)`; `WgpuBackend::from_instance(..)`.
4. Pick a non-sRGB format; configure the surface.
5. `backend.set_scale_factor(window.scale_factor())` and re-apply on
   `ScaleFactorChanged`; use **logical** coordinates for input.
6. `backend.set_clear_color(..)`.
7. Fonts: `backend.set_font_config(FontConfig { mode, device_pixel_rasterization })`
   (`FontMode::System` / `Pixel`). Then inject a matching measurer:
   `tree`/view `.set_text_measurer(Rc::new(BackendTextMeasurer { metrics:
   backend.text_metrics() }))` so layout measures the font that is painted.
8. On redraw: `update → layout → paint → into_draw_list`.
9. `surface.get_current_texture` → `begin_frame_with_view(view, w, h, format,
   viewport)` → `submit(&list)` → `end_frame` → `present`.
10. Map platform input to `InputEvent` and feed the view; `request_redraw()` when
    state changed.
11. Long work (network, disk) runs on a thread and returns through a winit
    `EventLoopProxy<..>`; the loop is asleep otherwise.

Backend-neutral alternative: `draw_backend_recording::RecordingBackend` records a
`DrawList` headlessly (used by `--selfcheck` and the benches). To size a window to
its content, `draw_ui::content_size(&tree, available)` gives the intrinsic size
(`deepseek_balance` uses it for the menu-bar panel).

## 7. Verify without a screenshot

The workspace rule is **no screenshot / screen recording**. Verify a frame by
reading its own data instead:

- Assert the `DrawList` command sequence, or paint into `RecordingBackend` and
  run `draw_profile::inspect` for structural errors (NaN geometry, unbalanced
  `Save`/`Restore`, command budget).
- Check semantic text/positions from the recorded commands.
- `examples/deepseek_balance/src/selfcheck.rs` is the model: `--selfcheck` records
  update → reply → layout → paint for both window and panel and asserts key
  content; `--dump-tree` / `--dump-commands` print the tree or the commands.

## 8. Conventions (规范)

- **Backend-neutral core.** `draw_core` / `draw_scene` / `draw_ui` /
  `draw_components` must not touch `web_sys`, `wgpu`, the DOM or the platform.
  Backends only consume `DrawList`.
- **One concern per module.** Views build trees; hosts own the platform loop and
  I/O; formatting/parsing live in small helpers, not in the widget code.
- **Theme is a value, tokens are the API.** Take `theme: Theme` in component
  constructors and use `theme.palette.*` / `theme.surface(SurfaceLevel::..)` /
  `space` / `radius` / `TextSize`; never hard-code hex. Dark is a token swap,
  not a second code path.
- **State is external.** View state in `Rc<Cell<_>>`/`Rc<RefCell<_>>` passed to
  callbacks; reach nodes by `NodeRef`, not by walking the tree.
- **Compose, then mount.** Build leaf components as locals, compose the root,
  mount once. Use low-level `tree.add_child` only for dynamic subtrees (overlays,
  router, runtime additions) and tools/tests/benches.
- **Only redraw when something changed**; the loop waits, and each handled input
  or result requests one frame.
- **Keep views headless-testable.** Behaviour from core state only; the same view
  paints into a real backend and a recording backend.

## 9. Where to look

| Need | File |
|---|---|
| Widget reference, layout, input, custom components | `docs/components.md` |
| Theme tokens & themed components | `docs/design-system.md` |
| Full UI app pattern | `examples/demo_app/src/lib.rs` |
| Minimal winit + wgpu host | `examples/wgpu_demo/src/app.rs` |
| Virtualized list, resize gutter, tabs, worker results | `examples/file_browser` |
| Photoshop-style editor using quill instead of egui (all tools + PNG import/export) | `examples/image_editor` |
| Menu-bar panel, two tabs, content-sized window, self-check | `examples/deepseek_balance` |
| Backends, adding one, browser boundary | `docs/backend.md` |

## 10. Agent cheat sheet — where to look

Read this section before scanning the repo; then open only the file you need.

- Build UI: `draw_components::{Flex, Row, Column, Card, Text, Button, Divider,
  List, ResizeHandle, Overlays, Router, NodeRef, Ref, set_text, set_on_click,
  set_on_drag, set_on_scroll}`.
- Mount/route/paint: `draw_scene::SceneChild::into_tree`,
  `SceneTree::{add_child, set_visible, update}`,
  `draw_ui::{layout, paint, route_input, set_text_measurer, mark_dirty, control,
  widget, content_size, hovered_cursor}`.
- Tokens: `draw_theme::{Theme, Tone, SurfaceLevel, space, radius, TextSize}`;
  construct `Theme::dark()` / `Theme::light()` and pass by value.
- Types/input: `draw_core::{Rect, Size, Vec2, Edges, Color, InputEvent,
  EventResult, Cursor, Key}`; IR/commands: `draw_render::{PaintContext,
  DrawList, DrawCommand}`.
- The five calls that answer most questions: `into_tree`, `layout`, `paint`,
  `route_input`, `set_text` — plus `Theme::palette`/`surface` for colours.
- Don't: invent a `quill::{Button, ..}` import (no facade), hard-code hex, store
  app state in the tree, read/write the tree from inside a callback without a
  shared cell, or call `tree.add_child` for static layout.
