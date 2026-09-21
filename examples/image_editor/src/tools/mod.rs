//! 工具系统。
//!
//! Phase 5 只有 [`BrushTool`]（绘画 / 擦除一个引擎）；Move / 框选 / 吸管在
//! Phase 8。工具是纯文档坐标上的逻辑，不认识 UI。

mod brush;
mod tool;

pub use brush::{BrushMode, BrushTool};
pub use tool::{PointerEvent, Tool, ToolContext};
