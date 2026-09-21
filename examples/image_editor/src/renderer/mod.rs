//! 合成器：把 [`Document`](crate::document::Document) 的图层栈画成一张像素图。
//!
//! §3.2 / §13：先定一个后端无关的 [`Renderer`] trait，第一版只有 CPU 实现。
//! 业务（文档、图层）不认识 wgpu；以后加 `WgpuRenderer` 时替换的是这个
//! 接口，而不是把渲染塞进文档模型。
//!
//! 现在是**对整张文档的合成**（每次重画全部目标像素），但每个图层只遍历
//! “落在目标里的那部分”，所以成本跟文档尺寸有关、不会随图层缓冲增长。
//!
//! 还可以更进一步：`DirtyRegion` 只重合成受影响的矩形（画笔一笔只有几百像素）。
//! 128×128 的文档目前没必要。

mod cpu;
mod target;

pub use cpu::{sample_pixel, CpuRenderer};
pub use target::RenderTarget;

use crate::document::Document;

/// 把文档合成到目标上。
pub trait Renderer {
    fn render(&self, document: &Document, target: &mut RenderTarget);
}
