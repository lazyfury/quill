//! 画笔 / 橡皮：同一个引擎，只有 [`BrushMode`] 不同（§11）。
//!
//! 一次 stroke 的入口是 `on_pointer_down`，持续在 `on_pointer_move`，在
//! `on_pointer_up` 结束。每次 move 只往当前图层的 `PixelBuffer` 写像素；
//! 落笔时先快照整层，抬笔时用 [`PixelRegion::diff`] 抓出差异区域，
//! 提交成**一条** [`PaintCommand`] —— 一笔 = 一步 undo。
//!
//! [`PixelRegion::diff`]: crate::document::PixelRegion::diff

use draw_core::Vec2;

use super::tool::{PointerEvent, Tool, ToolContext};
use crate::document::{Color, Document, LayerId, PaintCommand, PixelBuffer};

/// 画笔模式。擦除复用同一套 stamp / 插值逻辑。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushMode {
    Paint,
    Erase,
}

/// 落笔时记下的快照，抬笔时用它算出差异区域。
#[derive(Debug, Clone, PartialEq)]
struct PendingStroke {
    layer: LayerId,
    before: PixelBuffer,
}

/// 圆形画笔。尺寸是直径（文档像素），`opacity` 0..=1。
#[derive(Debug, Clone, PartialEq)]
pub struct BrushTool {
    /// 直径，文档像素。
    pub size: f32,
    /// 每一笔的不透明度。
    pub opacity: f32,
    pub color: Color,
    pub mode: BrushMode,
    drawing: bool,
    last: Option<Vec2>,
    /// 当前笔触落笔时的图层快照；不在笔画中时为 `None`。
    pending: Option<PendingStroke>,
}

impl BrushTool {
    pub const MIN_SIZE: f32 = 1.0;
    pub const MAX_SIZE: f32 = 200.0;

    /// 默认的绘画画笔（黑、12px、不透明）。
    pub fn paint() -> Self {
        Self {
            size: 12.0,
            opacity: 1.0,
            color: Color::BLACK,
            mode: BrushMode::Paint,
            drawing: false,
            last: None,
            pending: None,
        }
    }

    /// 橡皮构造器。运行时靠 `mode` 切换，这个构造器给测试 / 以后的
    /// “新建橡皮”用。
    #[allow(dead_code)]
    pub fn eraser() -> Self {
        Self {
            mode: BrushMode::Erase,
            ..Self::paint()
        }
    }

    /// 是否正处在一笔的中间。
    pub fn is_drawing(&self) -> bool {
        self.drawing
    }

    /// 把 `color` 也带上。运行时直接改 `self.brush.color`，这里给测试构造画笔用。
    #[allow(dead_code)]
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// 从上一个采样点到 `to` 画一段。第一个采样点（`last == None`）只压一次。
    pub fn stroke_to(&mut self, document: &mut Document, to: Vec2) {
        let Some(layer) = document.active_layer_mut() else {
            return;
        };
        let buffer = &mut layer.pixels;

        let last = self.last.unwrap_or(to);
        let delta = to - last;
        let distance = delta.length();
        // 步长取半径的一半，保证相邻 stamp 有重叠、线段不断开。
        let step = (self.size * 0.5 * 0.5).max(0.5);
        let steps = (distance / step).ceil().max(1.0) as u32;
        for sample in 1..=steps {
            let t = sample as f32 / steps as f32;
            self.dab(buffer, last + delta * t);
        }
        self.last = Some(to);
    }

    /// 在 `center` 压一个圆形 stamp。
    fn dab(&self, buffer: &mut PixelBuffer, center: Vec2) {
        let radius = (self.size * 0.5).max(0.5);
        let min_x = (center.x - radius - 1.0).floor() as i64;
        let max_x = (center.x + radius + 1.0).ceil() as i64;
        let min_y = (center.y - radius - 1.0).floor() as i64;
        let max_y = (center.y + radius + 1.0).ceil() as i64;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if x < 0 || y < 0 || x >= buffer.width as i64 || y >= buffer.height as i64 {
                    continue;
                }
                // 像素中心相对 stamp 圆心的距离；边缘 1px 抗锯齿。
                let dx = x as f32 + 0.5 - center.x;
                let dy = y as f32 + 0.5 - center.y;
                let distance = (dx * dx + dy * dy).sqrt();
                let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                let alpha = coverage * self.opacity;
                let (x, y) = (x as u32, y as u32);
                match self.mode {
                    BrushMode::Paint => buffer.blend_pixel(x, y, self.color, alpha),
                    BrushMode::Erase => buffer.erase_pixel(x, y, alpha),
                }
            }
        }
    }
}

impl Default for BrushTool {
    fn default() -> Self {
        Self::paint()
    }
}

impl Tool for BrushTool {
    fn name(&self) -> &'static str {
        match self.mode {
            BrushMode::Paint => "画笔",
            BrushMode::Erase => "橡皮",
        }
    }

    fn on_pointer_down(&mut self, ctx: &mut ToolContext, event: PointerEvent) {
        self.drawing = true;
        self.last = None;
        // 快照当前图层，抬笔时用于算差异区域（撤销只存这一块）。
        self.pending = ctx.document.active_layer().map(|layer| PendingStroke {
            layer: layer.id,
            before: layer.pixels.clone(),
        });
        self.stroke_to(ctx.document, event.position);
    }

    fn on_pointer_move(&mut self, ctx: &mut ToolContext, event: PointerEvent) {
        if self.drawing {
            self.stroke_to(ctx.document, event.position);
        }
    }

    fn on_pointer_up(&mut self, ctx: &mut ToolContext, _event: PointerEvent) {
        self.drawing = false;
        self.last = None;
        let Some(pending) = self.pending.take() else {
            return;
        };
        // 一笔成一条命令；整笔落在画布外等没有像素变化时不入栈。
        let command = match ctx.document.layer(pending.layer) {
            Some(layer) => {
                PaintCommand::capture(pending.layer, &pending.before, &layer.pixels, self.name())
            }
            None => None,
        };
        if let Some(command) = command {
            ctx.history.execute(Box::new(command), ctx.document);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::History;

    fn document() -> Document {
        Document::new("d", 32, 32)
    }

    /// 一个最小的工具上下文：文档 + 该测试自己的空历史。
    fn context<'a>(document: &'a mut Document, history: &'a mut History) -> ToolContext<'a> {
        ToolContext { document, history }
    }

    fn left(position: Vec2) -> PointerEvent {
        PointerEvent {
            position,
            button: draw_core::PointerButton::Left,
        }
    }

    fn at(document: &Document, x: u32, y: u32) -> Color {
        document.active_layer().unwrap().pixels.get_pixel(x, y)
    }

    #[test]
    fn a_single_dab_paints_the_center_of_the_layer() {
        let mut document = document();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        brush.stroke_to(&mut document, Vec2::new(16.0, 16.0));
        let color = at(&document, 16, 16);
        assert_eq!((color.r, color.g, color.b), (255, 0, 0));
        // 4px 半径之外（默认 12px 直径 -> r=6）还是背景。
        assert_eq!(at(&document, 30, 30), Color::WHITE);
    }

    #[test]
    fn erasing_lowers_alpha_instead_of_painting() {
        let mut document = document();
        let mut brush = BrushTool::eraser();
        brush.stroke_to(&mut document, Vec2::new(16.0, 16.0));
        assert_eq!(at(&document, 16, 16).a, 0);
        assert_eq!(at(&document, 30, 30).a, 255, "画布角落还在");
    }

    #[test]
    fn a_move_stroke_covers_the_points_between_samples() {
        let mut document = document();
        let mut history = History::new();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        brush.on_pointer_down(
            &mut context(&mut document, &mut history),
            left(Vec2::new(4.0, 16.0)),
        );
        brush.on_pointer_move(
            &mut context(&mut document, &mut history),
            left(Vec2::new(28.0, 16.0)),
        );
        // 两端之间的中间像素也被覆盖（插值有效）。
        for x in 4..=28 {
            assert_eq!(at(&document, x, 16).r, 255, "x = {x} 应该被线段覆盖");
        }
        assert!(brush.is_drawing());
    }

    #[test]
    fn opacity_blends_towards_the_background() {
        let mut document = document();
        let mut brush = BrushTool::paint().with_color(Color::BLACK);
        brush.opacity = 0.5;
        brush.stroke_to(&mut document, Vec2::new(16.0, 16.0));
        let color = at(&document, 16, 16);
        assert_eq!(color.r, color.g);
        assert!(color.r > 100 && color.r < 160, "r = {}", color.r);
    }

    #[test]
    fn out_of_bounds_stamps_are_clipped_not_panicking() {
        // 圆心在角落：一半在界外、一半在界内，界内那半应被画上。
        let mut inside = document();
        BrushTool::paint()
            .with_color(Color::RED)
            .stroke_to(&mut inside, Vec2::new(0.0, 0.0));
        assert_eq!(at(&inside, 0, 0), Color::RED);

        // 完全在界外：不 panic，也不动界内。
        let mut outside = document();
        BrushTool::paint()
            .with_color(Color::RED)
            .stroke_to(&mut outside, Vec2::new(-50.0, -50.0));
        assert_eq!(at(&outside, 0, 0), Color::WHITE);
    }

    #[test]
    #[ignore = "perf probe: --ignored --nocapture"]
    fn probe_stroke_cost() {
        use std::time::Instant;
        let mut document = Document::new("d", 800, 600);
        let mut brush = BrushTool::paint().with_color(Color::RED);
        let n = 200;
        let timer = Instant::now();
        for _ in 0..n {
            brush.last = None;
            brush.stroke_to(&mut document, Vec2::new(100.0, 100.0));
            brush.stroke_to(&mut document, Vec2::new(700.0, 500.0));
        }
        let ms = timer.elapsed().as_secs_f64() / n as f64 * 1000.0;
        eprintln!("stroke (600px, size 12) {ms:.3} ms");
    }

    #[test]
    fn a_full_stroke_begin_to_end_toggles_drawing() {
        let mut document = document();
        let mut history = History::new();
        let mut brush = BrushTool::paint();
        assert!(!brush.is_drawing());
        brush.on_pointer_down(
            &mut context(&mut document, &mut history),
            left(Vec2::new(8.0, 8.0)),
        );
        assert!(brush.is_drawing());
        brush.on_pointer_up(
            &mut context(&mut document, &mut history),
            left(Vec2::new(8.0, 8.0)),
        );
        assert!(!brush.is_drawing());
        assert_eq!(brush.last, None);
    }

    #[test]
    fn one_stroke_pushes_exactly_one_undo_step() {
        let mut document = document();
        let mut history = History::new();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        brush.on_pointer_down(
            &mut context(&mut document, &mut history),
            left(Vec2::new(4.0, 4.0)),
        );
        // 中间很多次 move，也只算**一笔**。
        for x in [8.0, 12.0, 16.0, 20.0] {
            brush.on_pointer_move(
                &mut context(&mut document, &mut history),
                left(Vec2::new(x, 4.0)),
            );
        }
        brush.on_pointer_up(
            &mut context(&mut document, &mut history),
            left(Vec2::new(20.0, 4.0)),
        );
        assert_eq!(history.undo_len(), 1);
    }

    #[test]
    fn undo_restores_the_pixels_and_redo_reapplies_them() {
        let mut document = document();
        let mut history = History::new();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        brush.on_pointer_down(
            &mut context(&mut document, &mut history),
            left(Vec2::new(16.0, 16.0)),
        );
        brush.on_pointer_up(
            &mut context(&mut document, &mut history),
            left(Vec2::new(16.0, 16.0)),
        );
        assert_eq!(at(&document, 16, 16), Color::RED);

        assert_eq!(history.undo(&mut document), Some("画笔"));
        assert_eq!(at(&document, 16, 16), Color::WHITE, "撤销回到落笔前");

        assert_eq!(history.redo(&mut document), Some("画笔"));
        assert_eq!(at(&document, 16, 16), Color::RED);
    }

    #[test]
    fn a_stroke_with_no_pixel_change_pushes_no_command() {
        let mut document = document();
        let mut history = History::new();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        // 整笔都在画布外：没有像素变化，不该占用一步撤销。
        brush.on_pointer_down(
            &mut context(&mut document, &mut history),
            left(Vec2::new(-100.0, -100.0)),
        );
        brush.on_pointer_up(
            &mut context(&mut document, &mut history),
            left(Vec2::new(-100.0, -100.0)),
        );
        assert!(!history.can_undo());
    }
}
