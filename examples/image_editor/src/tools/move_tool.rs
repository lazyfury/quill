//! 移动工具（Phase 8）：拖动改变**当前图层**相对文档原点的像素偏移。
//!
//! 只改 `Layer.position`，不动像素；合成器会把偏移算进摆放。和图层排序 / 增删
//! 一样，移动**不进撤销栈**（History 目前只有 `PaintCommand`）；一次拖拽只是把
//! 偏移设到新的整数点。

use draw_core::Vec2;

use super::tool::{PointerEvent, Tool, ToolContext};
use crate::document::{LayerId, Point, SetLayerPositionCommand};

/// 拖动当前图层。落笔记录图层与起点，移动时按指针位移设置 `position`。
#[derive(Debug, Clone, Default)]
pub struct MoveTool {
    layer: Option<LayerId>,
    /// 落笔时的文档坐标。
    origin: Vec2,
    /// 落笔时的图层位置。
    start: Point,
}

impl MoveTool {
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否正处在一次拖动中。
    pub fn is_moving(&self) -> bool {
        self.layer.is_some()
    }
}

impl Tool for MoveTool {
    fn name(&self) -> &'static str {
        "移动"
    }

    fn on_pointer_down(&mut self, ctx: &mut ToolContext, event: PointerEvent) {
        let Some(layer) = ctx.document.active_layer() else {
            self.layer = None;
            return;
        };
        self.layer = Some(layer.id);
        self.origin = event.position;
        self.start = layer.position;
    }

    fn on_pointer_move(&mut self, ctx: &mut ToolContext, event: PointerEvent) {
        let Some(id) = self.layer else {
            return;
        };
        let delta = event.position - self.origin;
        let position = Point::new(
            self.start.x + delta.x.round() as i32,
            self.start.y + delta.y.round() as i32,
        );
        ctx.document.set_layer_position(id, position);
    }

    fn on_pointer_up(&mut self, ctx: &mut ToolContext, _event: PointerEvent) {
        let Some(id) = self.layer.take() else {
            return;
        };
        let after = ctx
            .document
            .layer(id)
            .map(|layer| layer.position)
            .unwrap_or(self.start);
        // 一次拖动 = 一步 undo（只记录 position 的前后值）。
        if after != self.start {
            let command = SetLayerPositionCommand::new(id, self.start, after);
            ctx.history.execute(Box::new(command), ctx.document);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, History};
    use draw_core::PointerButton;

    fn left(position: Vec2) -> PointerEvent {
        PointerEvent {
            position,
            button: PointerButton::Left,
        }
    }

    #[test]
    fn a_move_drag_is_one_undo_step() {
        let mut document = Document::new("d", 32, 32);
        let mut history = History::new();
        let mut tool = MoveTool::new();
        let mut ctx = ToolContext {
            document: &mut document,
            history: &mut history,
        };
        tool.on_pointer_down(&mut ctx, left(Vec2::new(10.0, 10.0)));
        tool.on_pointer_move(&mut ctx, left(Vec2::new(15.0, 12.0)));
        tool.on_pointer_up(&mut ctx, left(Vec2::new(15.0, 12.0)));

        assert_eq!(document.active_layer().unwrap().position, Point::new(5, 2));
        assert_eq!(history.undo_len(), 1, "一次拖动只入一步");
        assert_eq!(history.undo(&mut document), Some("移动图层"));
        assert_eq!(document.active_layer().unwrap().position, Point::ZERO);
        assert_eq!(history.redo(&mut document), Some("移动图层"));
        assert_eq!(document.active_layer().unwrap().position, Point::new(5, 2));
    }

    #[test]
    fn dragging_moves_the_active_layer_by_the_pointer_delta() {
        let mut document = Document::new("d", 32, 32);
        let mut history = History::new();
        let mut tool = MoveTool::new();
        let mut ctx = ToolContext {
            document: &mut document,
            history: &mut history,
        };

        tool.on_pointer_down(&mut ctx, left(Vec2::new(10.0, 10.0)));
        assert!(tool.is_moving());
        tool.on_pointer_move(&mut ctx, left(Vec2::new(15.4, 6.6)));
        tool.on_pointer_up(&mut ctx, left(Vec2::new(15.4, 6.6)));
        assert!(!tool.is_moving());

        let layer = document.active_layer().unwrap();
        // 位移 (5.4, -3.4) -> 四舍五入成 (5, -3)。
        assert_eq!(layer.position, Point::new(5, -3));
    }

    #[test]
    fn a_drag_of_less_than_half_a_pixel_keeps_the_position() {
        let mut document = Document::new("d", 8, 8);
        let mut history = History::new();
        let mut tool = MoveTool::new();
        let mut ctx = ToolContext {
            document: &mut document,
            history: &mut history,
        };
        tool.on_pointer_down(&mut ctx, left(Vec2::new(4.0, 4.0)));
        tool.on_pointer_move(&mut ctx, left(Vec2::new(4.3, 4.2)));
        assert_eq!(document.active_layer().unwrap().position, Point::ZERO);
    }

    #[test]
    fn moving_a_document_without_layers_is_a_noop() {
        let mut document = Document::empty("d", 4, 4, crate::document::Color::WHITE);
        let mut history = History::new();
        let mut tool = MoveTool::new();
        let mut ctx = ToolContext {
            document: &mut document,
            history: &mut history,
        };
        tool.on_pointer_down(&mut ctx, left(Vec2::new(1.0, 1.0)));
        assert!(!tool.is_moving());
    }
}
