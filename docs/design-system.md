# Design system — theme & components

`draw_theme` holds the design tokens; `draw_kit` builds themed components on top
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

## Components — `draw_kit`

`Kit` layers themed chrome over a `draw_ui::Ui` without extending the core
`Widget` enum:

```rust
use draw_kit::{Card, Checkbox, Kit, Text, Tone};
use draw_theme::{space, Theme};
use draw_ui::Ui;

let mut ui = Ui::new();
let mut kit = Kit::new(Theme::dark());

let root = ui.root();
let card = kit.add(&mut ui, root, Card::new().gap(space::MD));
kit.add(&mut ui, card.id(), Text::heading("Settings"));
kit.add(&mut ui, card.id(), Text::small("Changes save automatically.").tone(Tone::Muted));
kit.add(&mut ui, card.id(), Checkbox::new("Verbose output"));

ui.layout(viewport);
kit.paint_surfaces(&ui, &mut ctx);   // behind content
ui.paint(&mut ctx);
kit.paint_foreground(&ui, &mut ctx); // marks, knobs, indicators
kit.handle_input(&ui, &event);       // alongside ui.handle_input(&event)
```

### Paint passes

`Kit` cannot extend `Ui::paint`, so hosts call three passes in order:

1. `kit.paint_surfaces` — rounded surfaces/borders behind content.
2. `ui.paint` — labels, buttons and other `draw_ui` widgets.
3. `kit.paint_foreground` — check marks, switch knobs, terminal dots, icons.

`paint::fill_rounded_rect` / `paint::surface` compose the render IR's rects and
circles into rounded surfaces without double-blending translucent fills.

### Interactions

`Kit::handle_input` hit-tests by walking up from `Ui::hit_test`, so clicking any
descendant of a component root activates it. Hover/pressed state propagates to
descendants, which lets indicators repaint from shared `Rc<Cell<_>>` state
without remounting. Checkbox/Switch share their state through `Rc<Cell<bool>>`.

Surfaces are usually static, but selection and hover need per-frame styles:
`Kit::dynamic_surface(node, |theme, state| ...)` recomputes a `SurfaceStyle` from
the theme and the node's `InteractState` on every frame.

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
| `Badge` | metadata tag; `tone`, `pill`, `solid`. |
| `Button` | `Primary`/`Secondary`/`Ghost` variants with `on_click`. |
| `CodeBlock` | code surface, optional filename/language. |
| `Terminal` | header dots, command and output lines. |
| `EmptyState` | icon placeholder, title, description. |
| `Checkbox` | compact control with shared state and `on_change`. |
| `Switch` | compact on/off control. |

Extend the library by implementing `draw_kit::Component`:

```rust
use draw_core::NodeId;
use draw_kit::{Component, ControlRef, Kit, Tone};
use draw_ui::Ui;

struct Caption(String);

impl Component for Caption {
    fn mount(self, kit: &mut Kit, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let _ = kit;
        ui.add(parent, draw_ui::Label::new(self.0))
    }
}
```

## Locked core

`draw_core`, `draw_scene`, `draw_render` and `draw_ui` are treated as a frozen
foundation for the design system. The theme and component layers only *use* their
public APIs:

- `draw_kit` composes `Panel`, `Label`, `Flex` and the layout setters.
- No new variants were added to `draw_ui::Widget`.
- Themed surfaces are painted by `draw_kit` into the backend-neutral
  `DrawList`, so every backend renders them.

If a future component truly requires a core change, do it as a separate,
backward-compatible addition and record it here.

## Deferred

Inputs, selects, tabs, tooltips, tables, lists, modals and toasts are staged
next. Rounded *strokes* are approximated by filling a border-colored rounded
rect and insetting the fill; a first-class rounded-rect command would be a
`draw_render` addition.
