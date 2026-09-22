//! `draw_ffi` — a C ABI over the backend-neutral core.
//!
//! A foreign-language host (the C++ `examples/cpp_ffi` demo is the first one)
//! builds its own scene/UI and needs exactly two things from quill:
//!
//! 1. the core value types (`Color`, `Vec2`, `Rect`, `Transform2D`) and
//! 2. a `DrawList` it can fill with backend-neutral commands.
//!
//! Both are exposed here. The host then walks the list through
//! [`quill_draw_list_command`] and rasterizes it with **its own** backend — the
//! C++ demo's OpenGL renderer. The UI itself is the host's business: nothing
//! from `draw_scene` / `draw_ui` crosses this boundary.
//!
//! ```text
//! C++ UI -> quill_draw_list_* -> DrawList -> C++ backend -> pixels
//! ```
//!
//! # Layout
//!
//! - [`types`] — the `#[repr(C)]` records and the opaque `QuillDrawList`.
//! - `convert` — marshalling between the core types and the records.
//! - `list` — the `extern "C"` allocation, builder and read-back functions.
//! - `tests` — the ABI contract (round-trip, null safety, version).
//!
//! `include/quill.h` is a hand-maintained mirror of the layout. Change one,
//! change the other, and bump [`ABI_VERSION`]. Details: `docs/cpp-ffi.md`.

mod convert;
mod list;
mod types;

#[cfg(test)]
mod tests;

pub use list::*;
pub use types::*;
