//! 合成器：把 [`Document`](crate::document::Document) 的图层栈画成一张像素图。
//!
//! §3.2 / §13：先定一个后端无关的 [`Renderer`] trait，第一版只有 CPU 实现。
//! 业务（文档、图层）不认识 wgpu；以后加 `WgpuRenderer` 时替换的是这个
//! 接口，而不是把渲染塞进文档模型。
//!
//! 现在是**全量合成**（每次从头合成整张图）。§25 允许 v1 先这样，脏矩形
//! 留给后续：
//!
//! TODO(v1.1): 增加 `DirtyRegion`，只重合成受影响的矩形。

mod cpu;
mod target;

pub use cpu::CpuRenderer;
pub use target::RenderTarget;

use crate::document::Document;

/// 把文档合成到目标上。
pub trait Renderer {
    fn render(&self, document: &Document, target: &mut RenderTarget);
}
