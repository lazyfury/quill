//! 文档数据模型：与 UI / 后端无关的纯数据。
//!
//! 这是 Phase 2 的核心：`Document -> Layer -> PixelBuffer -> Color`。它只依赖
//! `std`，不依赖 `draw_*` 或 `egui`，所以可以用 `cargo test` 直接验证，也符合
//! `AGENTS.md` 的依赖方向（`app -> document`，`document` 不反向依赖 UI）。
//!
//! 坐标/尺寸约定：
//!
//! - `Document.width/height` 是像素尺寸；`PixelBuffer` 的数据是行优先 RGBA。
//! - `Document.layers` 从下到上（`layers[0]` 是最底层），与 §13 合成顺序一致。
//! - `Layer.position` 是相对文档原点的像素偏移（Phase 3 的 Canvas 用）。
//!
//! 这里 `pub use` 出的是数据模型的公共表面；Phase 2 只有 `Document` 被 `app`
//! 直接用到，其余（`Color` / `Layer` / `PixelBuffer` …）要等 Phase 3/5 的
//! Canvas 与 Brush 才消费，所以先允许“未使用的再导出”。
#![allow(unused_imports)]

mod color;
// 结构与 §4 推荐目录一致（`document/document.rs`）；名字重复是刻意的。
#[allow(clippy::module_inception)]
mod document;
mod history;
mod id;
mod layer;
mod pixel_buffer;
mod point;
mod region;

pub use color::Color;
pub use document::{Document, DEFAULT_HEIGHT, DEFAULT_WIDTH};
pub use history::{
    AddLayerCommand, Command, CropLayerCommand, History, LayerMetaCommand, LayerStackMeta,
    PaintCommand, RemoveLayerCommand, SetLayerPositionCommand,
};
pub use id::{DocumentId, LayerId};
pub use layer::{BlendMode, Layer};
pub use pixel_buffer::PixelBuffer;
pub use point::Point;
pub use region::PixelRegion;

pub(crate) use pixel_buffer::blend_over;
