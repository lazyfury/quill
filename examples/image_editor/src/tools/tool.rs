//! 工具系统：可扩展的 [`Tool`] 接口 + 上下文。
//!
//! 工具只认识**文档坐标**和 `Document`，不认识任何 UI widget / 平台类型
//! （§24）。宿主 / 视图负责把屏幕坐标换算成文档坐标，再调工具。

use draw_core::{PointerButton, Vec2};

use crate::document::{Document, History};

/// 一次指针事件。`position` 是**文档坐标**（浮点，可为负 / 超出画布）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    pub position: Vec2,
    pub button: PointerButton,
}

/// 工具运行时能拿到的上下文。
///
/// 文档与撤销栈分开传，工具才能在画完一笔后把命令提交进历史（§Phase 6）。
pub struct ToolContext<'a> {
    pub document: &'a mut Document,
    pub history: &'a mut History,
}

/// 所有工具实现的接口（§9）。
///
/// `draw_overlay`（选框、笔刷圈的叠加绘制）等到有叠加层含义的工具（框选 /
/// 形状）再加，避免现在就把 UI 类型拖进工具层。
pub trait Tool {
    /// 工具名字，用于状态栏 / 日志。
    fn name(&self) -> &'static str;

    fn on_pointer_down(&mut self, ctx: &mut ToolContext, event: PointerEvent);
    fn on_pointer_move(&mut self, ctx: &mut ToolContext, event: PointerEvent);
    fn on_pointer_up(&mut self, ctx: &mut ToolContext, event: PointerEvent);
}
