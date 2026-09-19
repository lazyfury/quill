# Design system — theme & components

`draw_theme` holds the design tokens; `draw_components` builds themed components on top
of `draw_ui`. The core drawing crates are **locked** here: the theme and
component layers do not change `draw_core`, `draw_scene`, `draw_render` or
`draw_ui` (see the "Locked core" section below).

## Philosophy

Hierarchy comes from typography, spacing, alignment, borders, surface contrast
and interaction states — not decoration. The visual language is:

```
monochrome + thin borders + subtle surfaces + precise spacing
+ compact controls + restrained radius + strong typography
```

Accent colors are semantic only: success, warning, error, information, selection
and focus.

## Tokens — `draw_theme`

```rust
use draw_theme::{space, radius, control, Mode, TextSize, Theme};

let theme = Theme::dark();
theme.mode;                       // Mode::Dark
theme.palette.background;         // #0A0A0A
theme.palette.surface_raised;     // #171717
theme.palette.border;             // #262626
theme.surface(SurfaceLevel::Raised);
theme.spacing(space::LG);         // 16.0
```

### Color

| Token pair | Light | Dark |
|---|---|---|
| `background` | `#FFFFFF` | `#0A0A0A` |
| `foreground` | `#111111` | `#F5F5F5` |
| `surface` | `#FAFAFA` | `#111111` |
| `surface_raised` | `#F5F5F5` | `#171717` |
| `surface_hover` | `#F2F2F2` | `#1C1C1C` |
| `muted` | `#737373` | `#A3A3A3` |
| `subtle` | `#A3A3A3` | `#737373` |
| `border` | `#E5E5E5` | `#262626` |
| `border_subtle` | `#EEEEEE` | `#1F1F1F` |
| `code_surface` | `#F7F7F7` | `#111111` |

Semantic accents (`accent`, `success`, `warning`, `error`, `info`), `on_accent`,
`focus_ring` and `selection` round out [`Palette`]. Resolve a role with
`Palette::semantic(Semantic::Error)`.

### Scale

- Spacing: `space::{XXXS..COLOSSAL}` = `2,4,6,8,12,16,20,24,32,40,48,64,80`.
- Radius: `radius::{NONE,SM,MD,LG,PANEL,FULL}` = `0,4,6,8,10,9999`.
- Type: `TextSize::{Display,Title,Heading,Subheading,Body,Small,Caption}`.
- Controls: `control::{HEIGHT,HEIGHT_SM,HEIGHT_LG,ICON,ROW,ROW_SM,TAB}`.
- Motion: `motion::{FAST,NORMAL,SLOW}` = 100/150/200 ms.

## Components — `draw_components`

The crate split is deliberate: `draw_ui` is the UI runtime **and** the styling
primitives (`SurfaceStyle`, `fill_rounded_rect`/`inset`/`surface`, `Tone`,
`SurfaceTone`, and the `surface_decor`/`dynamic_surface_decor`/
`foreground_decor` factories), while `draw_components` contains **only component
builders**. Components implement `draw_app::Component`, receive the `Theme` as a
`Copy` value, and attach their chrome to their own node. Hosts build one tree
and use a single paint/input pass:

```rust
use draw_app::{Component, Flex};
use draw_components::{Card, Checkbox, Text};
use draw_scene::SceneTree;
use draw_theme::{space, Theme, Tone};

let theme = Theme::dark();
let mut tree = SceneTree::new();

let root = tree.add_child(tree.root(), Flex::column());
tree.add_child(root, Card::new(theme).gap(space::MD)
    .child(Text::heading("Settings", theme))
    .child(Text::small("Changes save automatically.", theme).tone(Tone::Muted))
    .child(Checkbox::new("Verbose output", theme)));

draw_ui::layout(&mut tree, viewport);
draw_ui::paint(&tree, &mut ctx);          // surfaces + content + marks, in tree order
draw_app::route_input(&mut tree, &event); // dispatches component clicks
```

The theme is a `Copy` value passed to constructors; nothing reads it from the
tree, so switching light/dark is just building with a different `Theme`.

### Paint passes

Themed chrome is attached to a control as a `draw_ui::NodeDecor` (built by the
`draw_ui` decorator helpers `surface_decor` / `dynamic_surface_decor` /
`foreground_decor`), so a single `ui.paint` runs it in tree order:

1. every decorator's `paint_behind` — rounded surfaces/borders behind content,
2. the control's own `Widget` content,
3. every decorator's `paint_front` — check marks, switch knobs, terminal dots.

There are no separate surface/foreground passes, and a decorator is painted
next to the node it belongs to (so it is torn down with the node).

`paint::fill_rounded_rect` / `paint::surface` compose the render IR's rects and
circles into rounded surfaces without double-blending translucent fills.

### Interactions

Components register clicks with `draw_app::set_on_click(tree, node, ..)` or the
`Component::on_click` builder; a hit on any
descendant walks up to the nearest ancestor callback. Hover/pressed/focused
state lives in the core and `Ui::state_for(node)` inherits it from ancestors,
which is what decorators read each frame. Checkbox/Switch share their state
through `Rc<Cell<bool>>`.

Surfaces are usually static, but selection and hover need per-frame styles:
`dynamic_surface_decor(|state| ...)` recomputes a `SurfaceStyle` from the node's
`InteractState` on every frame; the closure captures the theme/colors it needs.

### Demo

`demos/demo_app` is a three-column, macOS-style notes app built from these
components: sidebar (nav + selection), content list (note rows with thumbnail
placeholders) and detail pane (toolbar, hero scene, body, actions).

### Available components

| Component | Notes |
|---|---|
| `Text` | display/title/heading/subheading/body/small/caption; `tone`, `color`, wrapping. |
| `Card` | column flex container with themed surface + hairline border. |
| `Divider` | 1px horizontal/vertical rule. |
| `ResizeHandle` | draggable divider; resizes the target pane's flex basis. |
| `Badge` | metadata tag; `tone`, `pill`, `solid`. |
| `Button` | `Primary`/`Secondary`/`Ghost`/`Destructive` variants with `on_click`. |
| `CodeBlock` | code surface, optional filename/language. |
| `Terminal` | header dots, command and output lines. |
| `EmptyState` | icon placeholder, title, description. |
| `Checkbox` | compact control with shared state and `on_change`. |
| `Switch` | compact on/off control. |

`draw_components` containers take children, so a screen is one expression:

```rust
tree.add_child(tree.root(), Card::new(theme).gap(12.0)
    .child(Text::heading("Settings", theme))
    .child(Button::primary("Save", theme).on_click(save).grow(1.0)));
```

Every `Component` supports the same modifiers as a method: `grow`, `min_size`,
`anchors`/`offsets`, `background`/`surface`/`dynamic_background`, `foreground`,
`on_click`, `mouse_filter` and `child`.

Extend the library by implementing `draw_app::Component` (see
`docs/components.md` for the full `spec`/`widget` walkthrough).

## Overlays

`draw_components::Overlays` is a generic floating layer built on its own `Ui`.
It keeps the host pipeline explicit — the host lays out its UI, then the layer,
and paints the layer last:

```rust
app.ui.layout(viewport);
overlays.layout(&app.ui, viewport); // resolve targets after layout
// ... paint main UI ...
overlays.paint(&mut ctx);           // scrim + floating content on top
```

Input goes to the layer first; a modal entry returns `EventResult::Handled` so
the host must not process the event. With no open entries `handle_input` is a
no-op, so hosts can call it unconditionally.

| Builder | Behavior |
|---|---|
| `confirm(title, message)` | modal dialog, centered, scrim, Esc / click-outside / buttons close it. |
| `popover(target, placement, content)` | anchored to a laid-out control; `content` builds into the layer's `Ui`. |
| `tips(target, text)` | tooltip anchored to a control, shown only while it (or a descendant) is hovered. |
| `message(text)` / `message_tone(text, tone)` | transient toast, auto-dismissed after ~2.5s. |

Positioning is in `overlay::placement::place`: `Above`/`Below`/`Left`/`Right`
flip to the opposite side when they would leave the viewport, then clamp to an
8px margin; `Center`/`TopCenter`/`BottomCenter` are used for dialogs and toasts.
`Overlays::rect(id)` exposes the resolved rectangle for tests/tools.

Entries are declarative and rebuilt only when the set changes, so per-frame
layout stays incremental. Button clicks, Esc and click-outside push actions that
`handle_input` drains into `on_confirm` / `on_cancel` / `on_close` callbacks.

## Locked core

`draw_core`, `draw_scene`, `draw_render` and `draw_ui` are treated as a frozen
foundation for the design system. The theme and component layers only *use* their
public APIs:

- `draw_components` composes `Panel`, `Label`, `Flex` and the layout setters.
- No new variants were added to `draw_ui::Widget`.
- Themed surfaces are painted by `draw_components` into the backend-neutral
  `DrawList`, so every backend renders them.

If a future component truly requires a core change, do it as a separate,
backward-compatible addition and record it here.

### Recorded core additions

- **`draw_ui::NodeDecor` / `InteractState` + `Ui::add_decor`** (Stage 22): the
  closed `Widget` enum cannot carry themed chrome, so components attach a
  `NodeDecor` to a node instead. `Ui::paint` runs `paint_behind` / content /
  `paint_front` per node and `Ui::state_for` resolves inherited hover/pressed/
  focused. `Ui::set_on_click` now accepts any control (not just
  `Widget::Button`) and dispatches to the nearest ancestor callback, so themed
  component roots own their clicks. `draw_components` no longer keeps a surface /
  foreground / interaction registry; its decorator helpers build `NodeDecor`
  values from the theme. This is additive: existing `Widget`/`ControlData`
  shapes are unchanged.
- **`draw_ui` owns the theme and styling primitives** (Stage 24): `draw_ui` now
  receives a `Theme` value; it is not stored on the tree (the old `Ui::theme()`
  ambient theme was removed in Stage 25)
  tokens. `SurfaceStyle`, `fill_rounded_rect`/`fill_rounded_rect_corners`/`inset`/
  `surface`, `Tone`, `SurfaceTone` and the `surface_decor`/
  `dynamic_surface_decor`/`foreground_decor` factories moved from `draw_kit` into
  `draw_ui`. `Theme` stays pure data (mode + palette + scale accessors), and
  `draw_components` (renamed from `draw_kit`) now contains only component
  builders.

## Deferred

Rounded rectangles are now first-class `DrawCommand`s (`FillRoundedRect` /
`StrokeRoundedRect`) with per-corner radii (`CornerRadii`, so one shape can mix
square and rounded corners), implemented by the canvas, wgpu and recording
backends, so surfaces no longer compose circles + rects by hand. Inputs, selects,
tabs, tables and lists are staged next; see `docs/plan.md` for the full roadmap.
The overlay layer covers confirm dialogs, popovers, tooltips and toasts.
