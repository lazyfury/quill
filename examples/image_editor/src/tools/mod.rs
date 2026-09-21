//! 工具系统：在**文档坐标**上操作文档的纯逻辑，不认识 UI。
//!
//! [`BrushTool`]（画笔 / 擦除）与 [`MoveTool`]（移动图层）实现 [`Tool`]。
//! 框选与吸管不在这里：它们写的是**编辑器状态**（选区、前景色）而不是文档
//! 像素，视图直接用 [`crate::canvas::pixel_selection`] 与
//! [`crate::renderer::sample_pixel`] 完成。

mod brush;
mod move_tool;
mod tool;

pub use brush::{BrushMode, BrushShape, BrushTool};
pub use move_tool::MoveTool;
pub use tool::{PointerEvent, Tool, ToolContext};
