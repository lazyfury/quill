# cpp_ffi — a C++ UI + OpenGL backend on the quill core

This demo proves the FFI boundary from the other side: a **C++ program** builds
its own UI, asks the quill core for a backend-neutral `DrawList`, and rasterizes
it with a **C++ OpenGL 3.3 backend**. No Rust widget, layout, theme or backend
is involved.

```text
C++ UI (ui.cpp) --Canvas--> draw_ffi (Rust) --> DrawList --GlBackend--> pixels
```

## The boundary

`crates/draw_ffi` is the only Rust code in the picture. It exposes:

- the core value types (`QuillVec2`, `QuillRect`, `QuillColor`,
  `QuillTransform`, `QuillCornerRadii`, `QuillPaint`) and
- an opaque `QuillDrawList` with `quill_draw_list_*` builders and a flat
  `QuillCommand` record to read commands back.

The C++ host includes `crates/draw_ffi/include/quill.h` (a hand-maintained
mirror of the `#[repr(C)]` layout) and links the `libdraw_ffi.a` static library.
Nothing from `draw_scene` / `draw_ui` crosses the boundary — the UI is
organized in C++ on purpose.

ABI v1 covers the geometry and state commands. `DrawImage` / `DrawText` read
back as `QUILL_CMD_UNSUPPORTED` and are skipped; text would need the host to
rasterize glyphs, which is deliberately out of scope.

## Layout

| File | Concern |
|---|---|
| `src/canvas.{hpp,cpp}` | the only place `quill.h` appears; value types + `Canvas` |
| `src/theme.{hpp,cpp}` | the `draw_theme` tokens (dark palette + scales), mirrored for `demo_app` parity |
| `src/widget.{hpp,cpp}` | the `Widget` base and the `Column` / `Row` layout containers |
| `src/ui.{hpp,cpp}` | the dashboard (balance bar, chart, toggle, spinner) |
| `src/gallery.{hpp,cpp}` | a few `demo_app`-styled components (button / checkbox / switch / card / divider / badge) |
| `src/gl_backend.{hpp,cpp}` | the OpenGL backend (tessellation + state stack) |
| `src/native_window.mm` | the one Objective-C++ file: the NSView for wgpu |
| `src/main.cpp` | window/event loop, `--dump`, `--gallery`, `--demoapp`, `--wgpu`, `--selfcheck` |

## Build & run

Requires a C++17 toolchain, CMake ≥ 3.20 and macOS. GLFW is used if installed,
otherwise the pinned 3.4 release is fetched at configure time.

```bash
./build.sh                 # cargo build -p draw_ffi + demoapp_ffi + wgpu_ffi, then CMake
./build/cpp_ffi            # the live dashboard window
./build/cpp_ffi --gallery  # the component gallery, styled like demo_app
./build/cpp_ffi --demoapp  # the real demo_app gallery, loaded through demoapp_ffi
./build/cpp_ffi --wgpu     # render with the Rust wgpu backend instead of OpenGL
./build/cpp_ffi --dump     # print the DrawList command stream (no GPU)
./build/cpp_ffi --selfcheck
```

## Component gallery (`--gallery`)

The gallery is the like-for-like comparison with `examples/demo_app`. Its
components are reimplemented in C++ from the same tokens: `src/theme.cpp`
mirrors `draw_theme`'s dark `Palette` and `scale.rs`, and `src/gallery.cpp`
matches the geometry of `draw_components` (button height 36 / radius 6, checkbox
16×16 / radius 4, switch 34×18 / full radius, card radius 8 + hairline border,
1px divider, badge radius 4). ABI v1 has no text, so labels are drawn as
neutral bars — the chrome (fill, border, radius, sizes) is what to compare.

Run `./build/cpp_ffi --gallery` next to `cargo run -p wgpu_demo` and switch the
demo to its Surfaces / Controls groups.

## The real gallery (`--demoapp`)

`examples/demoapp_ffi` exposes the *actual* `demo_app` over a second C ABI. The
host creates a `DemoApp`, drives its frame (`demoapp_update` / `demoapp_layout`)
and paints it into a `DrawList` (`demoapp_paint`), then renders it with the same
OpenGL backend. Arrow keys switch the catalog group.

**Text is the caveat.** `DemoApp` emits a `DrawText` per label; ABI v1 has no
text record, so those read back as `QUILL_CMD_UNSUPPORTED` and are skipped. The
layout, colors and chrome are the real app's — the labels are missing. Run
`./build/cpp_ffi --dump --demoapp` to see it: 145 commands, 86 of them text.

## Two backends (`--wgpu`)

`examples/wgpu_ffi` wraps the existing Rust `draw_backend_wgpu` in a C ABI, so
the same `DrawList` can be rendered two ways:

```text
DrawList ─┬─> C++ GlBackend (OpenGL 3.3)   [default]
          └─> wgpu_ffi -> draw_backend_wgpu [--wgpu]
```

`--wgpu` creates the window with `GLFW_CLIENT_API = GLFW_NO_API` (no GL context)
and hands the `NSView` to Rust through `src/native_window.mm`; wgpu's Metal
backend attaches its own layer. `--selfcheck --wgpu` renders offscreen and reads
back with `wgpu_ffi_read_pixels`, asserting the same expected pixels as the
OpenGL check — the two backends are compared directly.

## Verification (no screenshots)

`--selfcheck` renders one frame into an offscreen framebuffer and reads it back
with `glReadPixels`, then asserts the pixels the layout promises. The dashboard
check covers the background, a panel fill, the bar's accent fill and its track;
`--selfcheck --gallery` covers the background, a card, and the primary /
destructive / hovered-secondary buttons. It also reports the command and
triangle counts. This is the backend's own pixel buffer, which is the project's
no-screenshot rule.

`--dump` is the GPU-free half: it proves the FFI + UI produced the expected
command stream. ABI mismatches are refused at startup via `quill_abi_version()`.
