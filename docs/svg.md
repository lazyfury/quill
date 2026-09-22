# draw_svg — backend-neutral SVG / icon packs

`crates/draw_svg` renders a small SVG subset into the quill IR. It is
backend-neutral (depends only on `draw_core` + `draw_render`) and has **no
external dependency**: it parses SVG itself and emits the existing `Line` and
`FillCircle` commands, so any backend can draw it.

## Why this shape

SVG is a vector format, not an image codec. Rather than link a heavy rasterizer
(`resvg`/`usvg`/`tiny-skia`) into the core, `draw_svg` flattens curves to
polylines and strokes them with primitives the IR already has:

```
SVG text -> SvgDocument (flattened polylines) -> PaintContext -> DrawList -> any backend
```

This keeps `cargo check --workspace` cheap and the core free of platform/GPU
concerns. It also means icons scale cleanly (they are re-tessellated at the
target size, not resampled from a bitmap).

## Supported

- **Elements:** `<svg>`, `<g>`, `<path>`, `<line>`, `<polyline>`, `<polygon>`,
  `<rect>` (incl. rounded), `<circle>`, `<ellipse>`.
- **Path data:** `M L H V C S Q T A Z`, absolute and relative, including compact
  number forms (`10-20`, `.5.5`) and arc flags.
- **Style** (inherited through `<g>` / `<svg>`): `stroke`, `stroke-width`,
  `stroke-linecap`, `stroke-linejoin`, plus the `viewBox`.
- **Colors:** `none`, `currentColor`, `#rgb` / `#rrggbb` / `#rrggbbaa`, and a few
  named colors.

## Not yet

- **Stroke only.** `fill` is parsed but not rendered, so fill-only artwork does
  not appear. This is exactly the Lucide case (`fill="none"` + `stroke`).
- `<style>` CSS, gradients, patterns, masks, filters, `<use>`, `transform`, and
  text are ignored. `<defs>`/`<symbol>`/`<clipPath>`/… contents are skipped.
- `stroke-linecap="square"` is drawn as butt (the IR `Line` has butt caps).

## API

```rust
use draw_core::{Color, Rect, Size, Vec2};
use draw_render::PaintContext;

let svg = draw_svg::SvgDocument::parse(source)?;
let mut ctx = PaintContext::new();
svg.draw(&mut ctx, Rect::from_min_size(Vec2::ZERO, Size::splat(24.0)), Color::BLACK);
```

`draw` maps the `viewBox` into the target rectangle with
`preserveAspectRatio="xMidYMid meet"` semantics (uniform scale, centred) and
resolves `stroke="currentColor"` from the color you pass. Because it writes
straight into a `PaintContext`, an icon can be drawn inside any `draw_ui`
decorator (via `draw_ui::foreground_decor`) without a texture or a raster cache.

`draw_svg::IconPack` indexes a directory tree of `.svg` files by file stem:

```rust
let pack = draw_svg::IconPack::open("assets/lucide/icons")?;
let brush = pack.load("brush").unwrap()?;
```

## Command cost

An icon is re-tessellated every frame, so `draw` skips work that cannot be seen:
zero-length segments and round joins that are within ~10° of straight (which is
most of the joints on a flattened arc). Drawing the whole 2112-icon Lucide pack
at 24×24 emits ~224 K commands (~106 per icon) rather than ~361 K without the
skips. Anti-aliasing comes from the wgpu backend's 4x MSAA, not from denser
geometry.

Consumer (the `image_editor` project, a sibling checkout): it uses it two ways — the toolbar's tool
and undo/redo buttons each build an `Icon` component (which strokes one SVG via a
foreground decorator; see `icons.rs` / `ui/toolbar.rs`), and a sidebar gallery
draws a grid of 20 (`icons.rs`). Setting `IMAGE_EDITOR_ICON_DIR` points the same
code at a full pack (2112 icons).

## Lucide

[Lucide](https://lucide.dev) is ISC-licensed and ships both SVGs and a font. The
`lucide-static` npm tarball is the easiest source:

```bash
curl -L -o lucide.tgz https://registry.npmjs.org/lucide-static/-/lucide-static-1.47.0.tgz
tar -xzf lucide.tgz package/icons   # 2112 icons
```

Validate the whole pack against the parser (ignored by default):

```bash
cargo test -p draw_svg -- --ignored --nocapture every_icon $PWD/package/icons
```

That test parses every icon, draws it at 24×24, and fails if any icon errors or
draws nothing. It is the parser's regression net for real-world inputs.

Vendoring icons requires keeping Lucide's `LICENSE` and the per-file
`@license lucide-static … ISC` header.
