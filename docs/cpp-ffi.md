# C ABI & the C++ host (`draw_ffi`, `examples/cpp_ffi`)

`draw_ffi` exposes the backend-neutral core to a non-Rust host over a C ABI.
The first consumer is `examples/cpp_ffi`: a C++ program that builds its own UI,
fills a quill `DrawList`, and rasterizes it with its own OpenGL 3.3 backend.

The point is the boundary, not the demo:

```text
C++ UI (ui.cpp) --Canvas--> draw_ffi --> DrawList --C++ GlBackend--> pixels
```

The UI is organized **in C++**. quill supplies only the core: the value types
and the command list. Nothing from `draw_scene` / `draw_ui` crosses the ABI, and
the OpenGL backend is C++, not a Rust `RenderBackend` implementation.

## What crosses the boundary

`crates/draw_ffi` depends on `draw_core` + `draw_render` only (no backend, no
scene/UI). Its `crate-type` is `staticlib` + `cdylib` + `rlib`, so a host links
`libdraw_ffi.a` (or the `.dylib`) and includes `include/quill.h`.

- **Value types** (`#[repr(C)]`, all `f32` / C enum): `QuillVec2`, `QuillRect`,
  `QuillColor`, `QuillTransform`, `QuillCornerRadii`, `QuillPaint`.
- **Opaque `QuillDrawList`**: `quill_draw_list_new/free/clear/len`, the state
  builders (`save`/`restore`/`set_transform`/`set_opacity`/`clip_rect`) and the
  geometry builders (`fill_rect`, `stroke_rect`, `line`, `fill_circle`,
  `stroke_circle`, `fill_rounded_rect`, `stroke_rounded_rect`).
- **Read-back**: `quill_draw_list_command(list, index)` returns a flat
  `QuillCommand` record — a `QuillCommandTag` plus every possible payload field.
  Unused fields are zeroed. A flat record (not a tagged union) is deliberate:
  the host is C++ with no generated union helpers, so `switch (cmd.tag)` over a
  record is the simplest safe shape.
- **Version**: `quill_abi_version()` must equal the header's `QUILL_ABI_VERSION`
  (the host refuses to run otherwise).

The header is a hand-maintained mirror of the `#[repr(C)]` layout. If you change
one, change the other and bump the version. `crates/draw_ffi/src/lib.rs` has
tests that pin the round-trip, the null-pointer safety and the version.

### Command-to-field map

| Tag | Fields |
|---|---|
| `SAVE` / `RESTORE` | — |
| `SET_TRANSFORM` | `transform` |
| `SET_OPACITY` | `opacity` |
| `CLIP_RECT` | `rect` |
| `FILL_RECT` | `rect`, `paint` |
| `STROKE_RECT` | `rect`, `paint`, `width` |
| `LINE` | `from`, `to`, `paint`, `width` |
| `FILL_CIRCLE` | `center`, `radius`, `paint` |
| `STROKE_CIRCLE` | `center`, `radius`, `paint`, `width` |
| `FILL_ROUNDED_RECT` | `rect`, `corners`, `paint` |
| `STROKE_ROUNDED_RECT` | `rect`, `corners`, `paint`, `width` |
| `UNSUPPORTED` | — (skip) |

## The C++ host

| File | Concern |
|---|---|
| `src/canvas.{hpp,cpp}` | the only place `quill.h` appears; value types + `Canvas` |
| `src/theme.{hpp,cpp}` | the `draw_theme` tokens, mirrored for `demo_app` parity |
| `src/widget.{hpp,cpp}` | the `Widget` base and `Column` / `Row` containers |
| `src/ui.{hpp,cpp}` | the dashboard |
| `src/gallery.{hpp,cpp}` | `demo_app`-styled components (button / checkbox / switch / card / divider / badge) |
| `src/gl_backend.{hpp,cpp}` | the OpenGL backend (tessellation + state stack) |
| `src/main.cpp` | window/event loop, `--dump`, `--gallery`, `--selfcheck` |

The OpenGL backend mirrors `draw_backend_wgpu`: transform, opacity and clip are
resolved on the CPU while tessellating, so the GPU pass is one flat
colored-triangle pipeline. `ClipRect` becomes `glScissor`, which is why a clip
change (or a `Restore` that changes one) flushes the current batch first.

```bash
./examples/cpp_ffi/build.sh
./examples/cpp_ffi/build/cpp_ffi --dump
./examples/cpp_ffi/build/cpp_ffi --selfcheck
./examples/cpp_ffi/build/cpp_ffi --gallery  # demo_app-styled components
./examples/cpp_ffi/build/cpp_ffi            # live window
```

`--gallery` renders a few components rebuilt from `draw_theme`'s tokens
(`src/theme.cpp`) and `draw_components`' geometry (`src/gallery.cpp`), so they
can be compared against `examples/demo_app` side by side. The dashboard and the
gallery share the same token source, so the whole demo is `demo_app`-styled.

## Loading the real `demo_app` (`--demoapp`)

`examples/demoapp_ffi` is a second C ABI, this time over the *actual* Rust
gallery. The host creates a `DemoApp`, drives its frame and reads back the
resulting `DrawList`:

```text
C++ host -> demoapp_new/set_viewport/update/layout/paint -> DemoApp -> DrawList -> C++ GlBackend
```

- `demoapp_new` / `demoapp_new_with_mode` / `demoapp_free`
- `demoapp_set_viewport`, `demoapp_show_group`, `demoapp_group_count`
- `demoapp_update`, `demoapp_layout`
- `demoapp_paint` -> a fresh `QuillDrawList*`, released with `quill_draw_list_free`

`draw_ffi` exposes a Rust helper, `wrap_draw_list(DrawList) -> *mut
QuillDrawList` (not part of the C ABI), so `demoapp_ffi` hands its list to the
same `quill_draw_list_command` read-back.

```bash
./build/cpp_ffi --demoapp            # arrow keys switch catalog group
./build/cpp_ffi --dump --demoapp     # 145 commands, 86 of them DrawText
./build/cpp_ffi --selfcheck --demoapp
```

**Text is the limitation.** `DemoApp` lays out with `draw_ui`'s built-in
`ApproxTextMeasurer` (it does not depend on `draw_font`) and emits a `DrawText`
for every label. ABI v1 has no text record, so those read back as
`QUILL_CMD_UNSUPPORTED` and the backend skips them: the geometry, layout and
colors are the real app's, the labels are missing. The `--dump` histogram makes
that visible (86 `unsupported` of 145 commands).

## The Rust wgpu backend over FFI (`--wgpu`)

`examples/wgpu_ffi` exposes the existing Rust `draw_backend_wgpu` over a C ABI,
so the C++ host can render the *same* `DrawList` with either backend:

```text
C++ builds a DrawList ─┬─> C++ GlBackend (OpenGL 3.3) ─> surface
                       └─> wgpu_ffi -> draw_backend_wgpu ─> surface
```

- `wgpu_ffi_new(view, w, h, scale)` — a native `NSView*` for a window surface,
  or null for a headless renderer.
- `wgpu_ffi_resize` / `wgpu_ffi_render` (surface + present) / `wgpu_ffi_free`.
- `wgpu_ffi_render_offscreen` + `wgpu_ffi_read_pixels` — the no-window
  verification path.

The host creates the window with `GLFW_CLIENT_API = GLFW_NO_API` for `--wgpu`
(no OpenGL context) and passes the view through the one Objective-C++ file,
`src/native_window.mm`. wgpu's Metal backend attaches its own layer.

```bash
./build/cpp_ffi --wgpu                       # Rust backend, live window
./build/cpp_ffi --selfcheck --wgpu           # offscreen readback
./build/cpp_ffi --selfcheck --wgpu --gallery
./build/cpp_ffi --selfcheck --wgpu --demoapp
```

The self-checks assert the *same* expected pixels for both backends, so
`--selfcheck` (OpenGL) and `--selfcheck --wgpu` are a direct comparison: the
same `DrawList`, two backends, identical output.

> One gotcha is encoded in `src/main.cpp`: `glReadPixels` is bottom-up, so the
> OpenGL check flips y; `wgpu_ffi_read_pixels` is already top-down. Two
> samplers, `sample` and `sample_top_down`.

## Verification (no screenshots)

- `--dump` builds the UI and prints the `DrawList` command stream with no GPU;
  it proves the FFI + C++ UI path.
- `--selfcheck` renders one frame into an offscreen framebuffer, reads it back
  with `glReadPixels`, and asserts the pixels the layout promises. The dashboard
  check covers the background, a panel fill, the bar's accent fill and its
  track; `--selfcheck --gallery` covers the background, a card, and the primary
  / destructive / hovered-secondary buttons; `--selfcheck --demoapp` checks the
  real gallery's sidebar surface and that the full frame arrived. `--selfcheck
  --wgpu` runs the same checks through `wgpu_ffi`'s offscreen readback. This is
  the backend's own pixel buffer, which is the project's no-screenshot rule.

The Rust side is covered by `cargo test -p draw_ffi`; `draw_ffi` is a root
workspace member, so `cargo test --workspace` includes it. The C++ side is not a
Cargo crate and has no `cargo` gate — `build.sh` + `--selfcheck` is its gate.

## Scope

- **ABI v1 has no text or images.** `DrawText` / `DrawImage` read back as
  `QUILL_CMD_UNSUPPORTED`. Text would require the host to rasterize glyphs
  (`draw_font` is Rust); a future ABI version could pass a glyph atlas + quads,
  but v1 keeps the demo self-contained and text-free.
- **The OpenGL host targets macOS.** It includes `<OpenGL/gl3.h>` directly.
  Porting it means adding a GL loader (GLAD) and the matching GLFW hints; the
  CMake fails with a clear message off macOS.
- **GLFW is fetched if absent.** `CMakeLists.txt` prefers an installed `glfw3`,
  otherwise it fetches the pinned 3.4 release at configure time.
