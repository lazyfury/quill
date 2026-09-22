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
use draw_theme::{
    compact_theme, default_theme, ControlSize, Mode, Space, SurfaceLevel, Theme,
};

let theme: &'static dyn Theme = default_theme(Mode::Dark);
theme.mode();                             // Mode::Dark
theme.palette().background;               // #0A0A0A
theme.palette().surface_raised;           // #171717
theme.palette().border;                   // #262626
theme.surface(SurfaceLevel::Raised);
theme.spacing(Space::LG);                 // 16.0
compact_theme(Mode::Dark).spacing(Space::LG); // 12.0 (0.75x)
theme.control_height(ControlSize::Mini);  // 32.0 comfortable / 24.0 compact
```

`Theme` is a trait (required: `palette()` + `mode()`; the rest are defaulted), so
an app can implement it for its own type and override any token. `DefaultTheme`
is the built-in implementation; `default_theme(..)` / `compact_theme(..)` return
`'static` trait objects ready to hand to components.

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
- Controls: `control::{HEIGHT,HEIGHT_SM,HEIGHT_LG,ICON,ROW,ROW_SM,TAB}` (the base / comfortable values).
- Motion: `motion::{FAST,NORMAL,SLOW}` = 100/150/200 ms.

### Density

`Theme` also exposes a `Density`: the spacing and control metrics components
read. `Density::COMFORTABLE` (default) is the base scale with regular controls;
`Density::COMPACT` tightens everything and makes controls mini.
`DefaultTheme::compact()` / `with_density(..)` swap it — a token swap, not a
second code path; colors and type sizes are unaffected.

| Token | Comfortable | Compact |
|---|---|---|
| `space_scale` (× every `Space`) | 1.0 | 0.75 |
| `control_height` | 36 | 28 |
| `control_height_mini` | 32 | 24 |
| `control_padding_x` / `_y` | 12 / 8 | 8 / 4 |
| `row_height` (lists / menus) | 36 | 28 |
| `default_control` | `Regular` | `Mini` |

Components read `theme.spacing(Space::…)`, `theme.control_height(size)`,
`theme.control_padding_x()/y()` and `theme.row_height()` rather than the `space`
/ `control` consts. `Button` takes a `ControlSize` (`Button::mini()` /
`Button::regular()`) and defaults to `theme.default_control()`; an explicit
`.min_size(..)` on a component wins over the density default (so a toolbar can
pin its icon buttons to a fixed size). A custom theme (e.g.
`image_editor::theme::editor_theme`) is just a `Theme` impl with a different
density.

## Components — `draw_components`

The crate split is deliberate: `draw_ui` is the UI runtime **and** the styling
primitives (`SurfaceStyle`, `fill_rounded_rect`/`inset`/`surface`, `Tone`,
`SurfaceTone`, and the `surface_decor`/`dynamic_surface_decor`/
`foreground_decor` factories), while `draw_components` contains **only component
builders**. Components implement `draw_components::Component`, receive a
`&'static dyn Theme`, and attach their chrome to their own node. Hosts build one
tree and use a single paint/input pass:

```rust
use draw_components::{Component, Flex};
use draw_components::{Card, Checkbox, Text};
use draw_scene::SceneTree;
use draw_theme::{default_theme, space, Mode, Theme, Tone};

let theme = default_theme(Mode::Dark);
let mut tree = SceneTree::new();

let root = tree.add_child(tree.root(), Flex::column());
tree.add_child(root, Card::new(theme).gap(space::MD)
    .child(Text::heading("Settings", theme))
    .child(Text::small("Changes save automatically.", theme).tone(Tone::Muted))
    .child(Checkbox::new("Verbose output", theme)));

draw_ui::layout(&mut tree, viewport);
draw_ui::paint(&tree, &mut ctx);          // surfaces + content + marks, in tree order
draw_ui::route_input(&mut tree, &event); // dispatches component clicks
```

The theme is a `&'static dyn Theme` passed to constructors; nothing reads it
from the tree, so switching light/dark is just building with a different theme.

### Paint passes

Themed chrome is attached to a control as a `draw_ui::NodeDecor` (built by the
`draw_ui` decorator helpers `surface_decor` / `dynamic_surface_decor` /
`foreground_decor`), so a single `draw_ui::paint` runs it in tree order:

1. every decorator's `paint_behind` — rounded surfaces/borders behind content,
2. the control's own `Widget` content,
3. every decorator's `paint_front` — check marks, switch knobs, terminal dots.

There are no separate surface/foreground passes, and a decorator is painted
next to the node it belongs to (so it is torn down with the node).

`paint::fill_rounded_rect` / `paint::surface` compose the render IR's rects and
circles into rounded surfaces without double-blending translucent fills.

### Interactions

Components register clicks with `draw_components::set_on_click(tree, node, ..)` or the
`Component::on_click` builder; a hit on any
descendant walks up to the nearest ancestor callback. Hover/pressed/focused
state lives in the core and `draw_ui::state_for(tree, node)` inherits it from ancestors,
which is what decorators read each frame. Checkbox/Switch share their state
through `Rc<Cell<bool>>`.

Surfaces are usually static, but selection and hover need per-frame styles:
`dynamic_surface_decor(|state| ...)` recomputes a `SurfaceStyle` from the node's
`InteractState` on every frame; the closure captures the theme/colors it needs.

### Demo

`examples/demo_app` is a **component gallery**: a sidebar of groups, a preview
`Router` (one page per group) and a two-column grid of live component cards.
The sidebar footer toggles light/dark by rebuilding the scene against the token
set, so the demo is itself the token-swap proof.

### Available components

| Component | Notes |
|---|---|
| `Text` | display/title/heading/subheading/body/small/caption; `tone`, `color`, wrapping. |
| `Card` | column flex container with themed surface + hairline border. |
| `Divider` | 1px horizontal/vertical rule. |
| `ResizeHandle` | draggable divider; resizes the target pane's flex basis; `invert()` when the target is on the far side (a right sidebar). |
| `Badge` | metadata tag; `tone`, `pill`, `solid`. |
| `Button` | `Primary`/`Secondary`/`Ghost`/`Destructive` variants with `on_click`; `ControlSize` via `mini()`/`regular()` (default from the theme density); an explicit `background`/`dynamic_background` overrides the variant surface. |
| `CodeBlock` | code surface, optional filename/language. |
| `Terminal` | header dots, command and output lines. |
| `EmptyState` | icon placeholder, title, description. |
| `Glyph` / `Icon` | in-code vector icons (check, cross, warning, search, chevrons, …) drawn from primitives — no SVG files. |
| `Checkbox` | compact control with shared state and `on_change`. |
| `Switch` | compact on/off control. |
| `List` | virtualized rows: mounts the viewport's rows (+1 buffer) and recycles them; `ListState` (`sync`/`scroll_by`/`scroll_to`/`invalidate`), wheel + click, container clip. |
| `ScrollView` | clip + offset viewport for arbitrary content with a draggable scrollbar; `ScrollViewState` (`sync`/`scroll_by`/`scroll_to`/`invalidate`), wheel + thumb drag, hidden when the content fits. |
| `Menu` / `MenuItem` | floating menu surface + rows (label, optional right-aligned shortcut, `tone`/`destructive`, `disabled`, `on_click`); `Menu::separator`/`min_width`; place with `Overlays::menu`. |

`draw_components` containers take children, so a screen is one expression:

```rust
tree.add_child(tree.root(), Card::new(theme).gap(12.0)
    .child(Text::heading("Settings", theme))
    .child(Button::primary("Save", theme).on_click(save).grow(1.0)));
```

Every `Component` supports the same modifiers as a method: `grow`, `min_size`,
`anchors`/`offsets`, `background`/`surface`/`dynamic_background`, `foreground`,
`on_click`, `mouse_filter`, `child`, `ref_` and `with_ref`.

Extend the library by implementing `draw_components::Component` (see
`docs/components.md` for the full `spec`/`widget` walkthrough).

## Overlays

`draw_components::Overlays` is a generic floating layer built on its own
`SceneTree`. It keeps the host pipeline explicit — the host lays out its UI,
then the layer, and paints the layer last:

```rust
draw_ui::layout(&mut tree, viewport);
overlays.layout(&tree, viewport); // resolve targets after layout
// ... paint main UI ...
overlays.paint(&mut ctx);         // scrim + floating content on top
```

Input goes to the layer first; a modal entry returns `EventResult::Handled` so
the host must not process the event. With no open entries `handle_input` is a
no-op, so hosts can call it unconditionally.

| Builder | Behavior |
|---|---|
| `confirm(title, message)` | modal dialog, centered, scrim, Esc / click-outside / buttons close it. |
| `popover(target, placement, content)` | anchored to a laid-out control; `content` builds into the layer's `SceneTree`. |
| `menu(target, content)` | drop-down menu: like `popover` but the content owns its chrome (add a `Menu`); anchored `BelowStart`, Esc / click-outside close it. |
| `tips(target, text)` | tooltip anchored to a control, shown only while it (or a descendant) is hovered. |
| `message(text)` / `message_tone(text, tone)` | transient toast, auto-dismissed after ~2.5s. |

Positioning is in `overlay::placement::place`: `Above`/`Below`/`Left`/`Right`
flip to the opposite side when they would leave the viewport, then clamp to an
8px margin; `BelowStart` left-aligns to the anchor (right-aligning when it would
overflow) for menus; `Center`/`TopCenter`/`BottomCenter` are used for dialogs and
toasts. `Overlays::rect(id)` exposes the resolved rectangle for tests/tools.

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

- **`draw_ui::NodeDecor` / `InteractState` + `draw_ui::add_decor`** (Stage 22):
  the closed `Widget` enum cannot carry themed chrome, so components attach a
  `NodeDecor` to a node instead. `draw_ui::paint` runs `paint_behind` / content /
  `paint_front` per node and `draw_ui::state_for` resolves inherited hover/
  pressed/focused. `draw_components::set_on_click` now accepts any control (not
  just `Widget::Button`) and dispatches to the nearest ancestor callback, so
  themed component roots own their clicks. `draw_components` no longer keeps a surface /
  foreground / interaction registry; its decorator helpers build `NodeDecor`
  values from the theme. This is additive: existing `Widget`/`ControlData`
  shapes are unchanged.
- **`draw_ui` owns the styling primitives** (Stage 24; the theme was later
  reverted to a constructor argument in Stage 25): `SurfaceStyle`,
  `fill_rounded_rect`/`fill_rounded_rect_corners`/`inset`/`surface`, `Tone`,
  `SurfaceTone` and the `surface_decor`/`dynamic_surface_decor`/
  `foreground_decor` factories moved from `draw_kit` into `draw_ui`.
  `draw_components` (renamed from `draw_kit`) contains only component builders;
  components receive the theme as a `&'static dyn Theme`.
- **`draw_components::NodeRef` / `Ref<C>` + `Component::ref_` / `with_ref`**
  (Stage 25.x): a component is a pure spec with no identity until mount, so the
  declarative chain exposes node ids through callback refs. `NodeRef` is a clone
  slot filled at mount; `Component::ref_(&slot)` and `Component::with_ref(cb)`
  wrap a component in `Ref<C>`, which implements both `Component` and
  `draw_scene::SceneChild`, so the slot is filled identically whether mounted via
  `SceneTree::add_child(parent, c.ref_(&slot))` or `parent.child(c.ref_(&slot))`.
  This is the Godot `Node*`-from-`new()` / React `ref` equivalent; it is additive
  and does not change `Widget`/`ControlData`/`Spec` shapes.
- **`SceneTree::from_component` / `SceneChild::into_tree`** (Stage 25.x):
  `draw_scene` gained a top-level constructor — `SceneTree::from_component(root)`
  mounts a `SceneChild` under the root of a fresh tree, and `SceneChild::into_tree()`
  is the chainable sugar. `SceneTree::add_child` stays as the low-level primitive.
  `SceneChild` is now explicitly `Sized` (it already took `self` by value). A whole
  scene can therefore compose declaratively with `Component::child` and mount once.
  `ResizeHandle::target` changed from an eager `NodeId` to a deferred `NodeRef`, so
  a divider can reference its sibling pane before mount (order-independent); only
  the demo called `.target`.
- **`draw_ui::content_size`** (driven by `examples/deepseek_balance`):
  `draw_ui::layout` pins every UI root to the viewport, so a view always fills the
  surface it is handed and no resolved rectangle says how much room the content
  *wanted*. A host that sizes its window to its content — the menu-bar panel in
  `examples/deepseek_balance`, via `Window::request_inner_size` — needs exactly
  that, and it has to come from the same `TextMeasurer` that will paint the frame
  (sizing against one font and drawing with another is how text gets clipped).
  `content_size(tree, available)` exposes the measure pass on its own: no
  painting, no backend, no window, and the result includes each root's own
  padding. Additive — no `Widget`/`ControlData` shape changed; the measurement
  it caches is cleared by the next `layout`. It reads the tree's UI state, so it
  is for trees built through `draw_ui` (a built view already has that state).
- **Clipping + wheel routing + `ScrollCallback`** (Stage 25.14, the `List`
  component): `DrawCommand::ClipRect` existed in the IR and in all three
  backends but `draw_ui` never emitted it, so nothing could be clipped and no
  scrolling control was possible. `ControlData.clip` (opt-in) is now the only
  source of a clip: layout resolves it in the same pre-order pass that writes
  the rectangles back (`ControlData.clip_rect`, intersected with the nearest
  clipping ancestor's, `Rect::ZERO` when the intersection is empty), paint
  pushes one `save` + `clip_rect` per clipped region and pops it with `restore`,
  and hit testing refuses points outside `clip_rect`. Everything is gated on a
  single `tree.iter().any(clip)` scan, so a tree that clips nothing pays
  nothing and emits no clip commands. `InputEvent::Wheel { position, delta }`
  was a variant nothing consumed: `handle_input` now hit-tests it and routes it
  to the nearest ancestor with a `Control::scroll_callback`
  (`draw_components::set_on_scroll` / `Component::on_scroll`, the wheel counterpart of
  `on_click`/`on_drag`), returning `Handled` only when something took it.
  Additive: no `Widget` variant added, existing `ControlData` fields unchanged.
- **Wrapping text min-width capped by the offered width** (driven by
  `examples/deepseek_balance`): `Widget::Label`/`Widget::Button` reported their
  min-content width as the widest *unbreakable unit* (`longest_unit_width`).
  The paint pass already hard-breaks an overlong word (`layout::text`), but the
  measure pass did not know that, so one giant token — the one-line JSON body
  of an API error reply, a long URL — reported a min wider than the viewport.
  Flex cannot shrink below a child's min, so the whole column stretched and
  siblings (the header row's refresh button) were pushed off the surface;
  `content_size` reported the same inflated min and a fit-to-content window
  grew with it. A wrapping label now caps its min (and thus preferred) width at
  the width the parent offered, matching what paint can actually do; labels
  with `wrap: false` keep reporting the true unbreakable width. Behavior
  changes only in the pathological case; no `Widget`/`ControlData` shape
  changed.
- **`TextOptions::word_break` / `WordBreak`** (Stage 25.x): text wrapping
  previously had a single fixed strategy — words (whitespace-delimited Latin
  runs) break as units, CJK wide characters break individually, and an overlong
  unit hard-breaks per character. That behavior is now configurable via
  `TextOptions.word_break: WordBreak` (`Word` = the prior behavior, `BreakAll` =
  every character is a break opportunity, `KeepAll` = only whitespace breaks;
  CJK joins the surrounding word). `wrap_text_with_break` and the `tokens`
  splitter take the mode; `longest_unit_width_with` takes it so min-content
  sizing tracks the same break granularity. `Text` and `Label` gain a
  `word_break` builder. Additive and backward compatible: `WordBreak::Word` is
  the `Default`, so every existing `TextOptions` literal/`default()` call keeps
  the old output.
- **Font weight** (Stage 25.x): text had no weight knob — `DrawCommand::DrawText`,
  `Widget::Label`/`ButtonData`, and the theme all stopped at `font_size`.
  `draw_core::FontWeight` (now numeric, 100–900; `NORMAL`/`BOLD`/`MEDIUM`/…)
  flows through the IR, the UI and the theme. `TextOptions` gains `weight` (so
  the paint-side text cache keys on it), `TextMeasurer` gains
  `advance_weighted` / `measure_line_weighted` / `measure_run_weighted` with
  regular-metrics defaults (existing measurers keep compiling), and
  `Theme::font_weight(TextSize)` gives a per-role token (default `Normal`, so
  nothing changes visually unless a theme or component asks for bold). `Text`
  and `Button` gain `.weight(..)` / `.bold()`; `Label` gains `.weight(..)`.
  Backend-facing: `PaintContext::draw_text_weighted`, Canvas passes the numeric
  weight into the CSS font shorthand, and `draw_font::FontServer` resolves it to
  the nearest face (shared atlas). Additive and backward compatible: `draw_text`
  still draws `Normal`, and all existing `TextOptions` constructors default to
  `Normal`. See [`docs/font.md`](font.md).
- **Wrapping flex containers report the stacked cross size** (driven by
  `image_editor`'s new-document preset row): `measure_flex` computed a
  container's preferred cross size from the tallest single item even when `wrap`
  was on, so a row that broke into two lines still reported one line's height.
  The container kept that height and the second line overlapped the sibling below
  it. Measure now breaks the items into lines against the offered main size (the
  same `wrap_lines` partition `arrange_flex` uses) and sums the per-line cross
  maxima plus `cross_gap`, for horizontal and vertical wrap alike. Only wrapping
  containers change (they get the height they actually occupy); non-wrapping flex
  is untouched.
- **`Overlays::menu` + `Placement::BelowStart`** (Stage 25, image editor menu
  bar): the overlay layer could anchor a `popover` but had no menu semantics and
  no left-aligned placement. `Overlays::menu(target, content)` is a new overlay
  kind whose content owns all chrome (no default surface/padding wrapper),
  anchored `BelowStart` (left edges aligned; right-aligns near the right edge),
  and dismissed by Escape / click-outside like a popover. Additive: the existing
  `popover`/`confirm`/`tips`/`message` entries and the `Placement` variants are
  unchanged. `Menu`/`MenuItem` themselves are plain `draw_components` themed
  components — no `Widget`/`ControlData` shape changed.
- **`Theme.density` (`Density`)** (driven by `image_editor`'s compact theme):
  spacing and control metrics became a token instead of hardcoded `draw_theme`
  consts, so a custom theme can swap them without a second code path. See
  §Density above for the token list, accessors and the `compact()` swap.
- **`ResizeHandle::invert()`** (driven by `image_editor`'s resizable
  right sidebar): the handle assumed its target pane was on the *near* side, so
  a right-hand sidebar (target on the far side) resized in the wrong direction.
  `invert()` flips the drag delta. Additive — the default behavior and every
  existing call site are unchanged.
- **`Control::pointer_callback` / `Component::on_pointer`** (driven by
  `image_editor`'s colour picker): `DragCallback` only reports a
  **delta**, so a component could not map the pointer onto its own rectangle
  (sliders, colour pickers). `Control` gains an additive
  `pointer_callback: Option<Rc<RefCell<dyn FnMut(Rect, Vec2)>>>` fired on press
  and on every move while held, with the control's rect and the absolute pointer
  position; `draw_components` exposes it as `Component::on_pointer` /
  `set_pointer_callback`. `draw_ui::handle_input` routes the held pointer to the
  pressed control after drag capture. Additive: `Widget`/`ControlData` shapes
  are unchanged and existing `on_click`/`on_drag` callbacks are untouched.

## Deferred

Rounded rectangles are now first-class `DrawCommand`s (`FillRoundedRect` /
`StrokeRoundedRect`) with per-corner radii (`CornerRadii`, so one shape can mix
square and rounded corners), implemented by the canvas, wgpu and recording
backends, so surfaces no longer compose circles + rects by hand. Inputs, selects,
tabs and tables are staged next; see `docs/plan.md` for the full roadmap.
The overlay layer covers confirm dialogs, popovers, menus, tooltips and toasts,
and `List` covers scrolling rows (`docs/components.md`).
