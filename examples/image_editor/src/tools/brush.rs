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
use crate::document::{Color, Document, LayerId, PaintCommand, PixelBuffer, PixelRegion, Point};

/// 画笔模式。擦除复用同一套 stamp / 插值逻辑。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushMode {
    Paint,
    Erase,
}

/// 笔刷形状。`Square` 更适合像素画（偶数尺寸是整数边长，不会出十字 / 多一格）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrushShape {
    Round,
    #[default]
    Square,
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
    /// 可选选区：只有落在里面的像素会被写（Phase 8 的框选裁剪）。
    /// 视图在落笔前从编辑器状态同步过来。
    pub clip: Option<PixelRegion>,
    /// 像素模式：边缘不做抗锯齿（`coverage` 只有 0 / 1）。
    pub hard: bool,
    /// 笔刷形状。
    pub shape: BrushShape,
    drawing: bool,
    last: Option<Vec2>,
    /// 当前笔触落笔时的图层快照；不在笔画中时为 `None`。
    pending: Option<PendingStroke>,
}

impl BrushTool {
    pub const MIN_SIZE: f32 = 1.0;
    pub const MAX_SIZE: f32 = 200.0;

    /// 默认的绘画画笔（黑、1px、不透明、硬边方形）。
    ///
    /// 1px + 硬边 + 最近邻过滤是像素图默认值：放大后画出的就是硬边单像素。
    pub fn paint() -> Self {
        Self {
            size: 1.0,
            opacity: 1.0,
            color: Color::BLACK,
            mode: BrushMode::Paint,
            clip: None,
            hard: true,
            shape: BrushShape::Square,
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
    ///
    /// `to` / `last` 是**文档坐标**；写入时按当前图层的 `position`（缓冲区原点）
    /// 换算到缓冲区索引，所以笔迹始终对着光标。
    pub fn stroke_to(&mut self, document: &mut Document, to: Vec2) {
        let Some(layer) = document.active_layer_mut() else {
            return;
        };
        let offset = layer.position;
        let buffer = &mut layer.pixels;

        let last = self.last.unwrap_or(to);
        // 1px 硬边笔走 Bresenham：每个像素只压一次，斜线不会出 L 型加粗。
        if self.hard && self.size <= 1.0 {
            self.stroke_line_pixels(buffer, offset, last, to);
            self.last = Some(to);
            return;
        }
        let delta = to - last;
        let distance = delta.length();
        // 步长取半径的一半，保证相邻 stamp 有重叠、线段不断开。
        let step = (self.size * 0.5 * 0.5).max(0.5);
        let steps = (distance / step).ceil().max(1.0) as u32;
        for sample in 1..=steps {
            let t = sample as f32 / steps as f32;
            self.dab(buffer, offset, last + delta * t);
        }
        self.last = Some(to);
    }

    /// 用 Bresenham 把文档像素 `from -> to` 连成一条 1px 线（硬边笔用）。
    fn stroke_line_pixels(&self, buffer: &mut PixelBuffer, offset: Point, from: Vec2, to: Vec2) {
        let (mut x, mut y) = (from.x.floor() as i64, from.y.floor() as i64);
        let (target_x, target_y) = (to.x.floor() as i64, to.y.floor() as i64);
        let dx = (target_x - x).abs();
        let dy = -(target_y - y).abs();
        let step_x = if x < target_x { 1 } else { -1 };
        let step_y = if y < target_y { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.stamp_doc_pixel(buffer, offset, x, y);
            if x == target_x && y == target_y {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += step_x;
            }
            if e2 <= dx {
                err += dx;
                y += step_y;
            }
        }
    }

    /// 往文档坐标 `(doc_x, doc_y)` 对应的图层像素压一个实心点（硬边 1px 线用）。
    fn stamp_doc_pixel(&self, buffer: &mut PixelBuffer, offset: Point, doc_x: i64, doc_y: i64) {
        if let Some(clip) = self.clip {
            if doc_x < 0 || doc_y < 0 || !clip.contains(doc_x as u32, doc_y as u32) {
                return;
            }
        }
        let bx = doc_x - offset.x as i64;
        let by = doc_y - offset.y as i64;
        if bx < 0 || by < 0 || bx >= buffer.width as i64 || by >= buffer.height as i64 {
            return;
        }
        let (bx, by) = (bx as u32, by as u32);
        match self.mode {
            BrushMode::Paint => buffer.blend_pixel(bx, by, self.color, self.opacity),
            BrushMode::Erase => buffer.erase_pixel(bx, by, self.opacity),
        }
    }

    /// 在文档坐标 `center` 压一个 stamp；落在 `offset`（图层缓冲区原点）
    /// 之外的像素被裁掉。
    fn dab(&self, buffer: &mut PixelBuffer, offset: Point, mut center: Vec2) {
        // 像素模式（hard）：圆心吸附到光标下的像素中心，对所有尺寸生效。
        if self.hard {
            center = Vec2::new(center.x.floor() + 0.5, center.y.floor() + 0.5);
        }
        let radius = (self.size * 0.5).max(0.5);
        // 方形硬边的整数边长：奇数 n×n 居中，偶数把光标像素放左上（避免多一格）。
        let side = self.size.round().max(1.0) as i64;
        let half = side / 2;
        let square_min = -half + i64::from(side % 2 == 0);
        let square_max = half;
        let center_px = (center.x.floor() as i64, center.y.floor() as i64);

        let min_x = (center.x - radius - 1.0).floor() as i64;
        let max_x = (center.x + radius + 1.0).ceil() as i64;
        let min_y = (center.y - radius - 1.0).floor() as i64;
        let max_y = (center.y + radius + 1.0).ceil() as i64;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                // 选区 / 距离都在文档坐标里算。
                if let Some(clip) = self.clip {
                    if x < 0 || y < 0 || !clip.contains(x as u32, y as u32) {
                        continue;
                    }
                }
                let dx = x as f32 + 0.5 - center.x;
                let dy = y as f32 + 0.5 - center.y;
                let coverage = if self.hard {
                    match self.shape {
                        BrushShape::Round => {
                            if (dx * dx + dy * dy).sqrt() <= radius {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        BrushShape::Square => {
                            let ix = x - center_px.0;
                            let iy = y - center_px.1;
                            if ix >= square_min
                                && ix <= square_max
                                && iy >= square_min
                                && iy <= square_max
                            {
                                1.0
                            } else {
                                0.0
                            }
                        }
                    }
                } else {
                    // 软边：圆用欧氏距离，方形用切比雪夫距离（软方形）。
                    let metric = match self.shape {
                        BrushShape::Round => (dx * dx + dy * dy).sqrt(),
                        BrushShape::Square => dx.abs().max(dy.abs()),
                    };
                    (radius + 0.5 - metric).clamp(0.0, 1.0)
                };
                if coverage <= 0.0 {
                    continue;
                }
                // 文档坐标 -> 图层缓冲区索引。
                let bx = x - offset.x as i64;
                let by = y - offset.y as i64;
                if bx < 0 || by < 0 || bx >= buffer.width as i64 || by >= buffer.height as i64 {
                    continue;
                }
                let alpha = coverage * self.opacity;
                let (bx, by) = (bx as u32, by as u32);
                match self.mode {
                    BrushMode::Paint => buffer.blend_pixel(bx, by, self.color, alpha),
                    BrushMode::Erase => buffer.erase_pixel(bx, by, alpha),
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
        // 落笔前把当前图层缓冲区扩到覆盖整张文档：笔迹落在光标下（选区外的
        // 文档区域也能画），并且移出画布的内容保留在缓冲区里、不裁掉。
        if let Some(id) = ctx.document.active_layer().map(|layer| layer.id) {
            let (dx, dy) = ctx.document.ensure_layer_covers_document(id);
            // 缓冲区坐标整体平移了，旧命令记录的区域要跟着平移。
            ctx.history.translate_layer(id, dx, dy);
        }
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
        // 相邻像素还是背景（1px 笔只填光标下的那个像素）。
        assert_eq!(at(&document, 30, 30), Color::WHITE);
    }

    #[test]
    fn a_one_pixel_brush_snaps_to_the_pixel_under_the_cursor() {
        let mut document = document();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        // 落点在像素 (10, 20) 内部而非中心：像素笔应实心填满该像素、无灰边。
        brush.stroke_to(&mut document, Vec2::new(10.3, 20.7));
        assert_eq!(at(&document, 10, 20), Color::RED);
        assert_eq!(at(&document, 9, 20), Color::WHITE, "左邻不沾");
        assert_eq!(at(&document, 10, 19), Color::WHITE, "上邻不沾");
        assert_eq!(at(&document, 11, 21), Color::WHITE, "右下不沾");
    }

    #[test]
    fn hard_edges_paint_only_solid_pixels() {
        let mut document = document();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        brush.size = 3.0;
        brush.stroke_to(&mut document, Vec2::new(16.0, 16.0));
        // 硬边方形 3×3：全是实心红，没有半透明边。
        for y in 15..=17 {
            for x in 15..=17 {
                assert_eq!(at(&document, x, y), Color::RED, "({x}, {y})");
            }
        }
        assert_eq!(at(&document, 14, 16), Color::WHITE);
        assert_eq!(at(&document, 18, 16), Color::WHITE);
    }

    #[test]
    fn the_soft_brush_keeps_anti_aliased_edges() {
        let mut document = document();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        brush.size = 3.0;
        brush.hard = false;
        brush.stroke_to(&mut document, Vec2::new(16.0, 16.0));
        // 负向对照：软笔会产生中间色（既不是纯红也不是纯白）。
        let partial = (14..=18).any(|y| {
            (14..=18).any(|x| {
                let color = at(&document, x, y);
                color != Color::RED && color != Color::WHITE && color.a != 0
            })
        });
        assert!(partial, "软笔应产生抗锯齿边");
    }

    #[test]
    fn an_even_square_brush_of_size_two_paints_four_pixels() {
        let mut document = document();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        brush.size = 2.0;
        brush.stroke_to(&mut document, Vec2::new(16.0, 16.0));
        // n=2：光标像素放左上，(16,16)..(17,17)。
        for (x, y) in [(16, 16), (17, 16), (16, 17), (17, 17)] {
            assert_eq!(at(&document, x, y), Color::RED, "({x}, {y})");
        }
        assert_eq!(at(&document, 15, 16), Color::WHITE);
        assert_eq!(at(&document, 18, 16), Color::WHITE);
    }

    #[test]
    fn a_one_pixel_hard_line_is_a_clean_bresenham_line() {
        let mut document = document();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        brush.stroke_to(&mut document, Vec2::new(4.0, 4.0));
        // 45° 斜线：正好 8 个像素，不会因重复压角 / 插值而加粗成 L 型。
        brush.stroke_to(&mut document, Vec2::new(11.0, 11.0));
        let painted: Vec<(u32, u32)> = (0..32)
            .flat_map(|y| (0..32).map(move |x| (x, y)))
            .filter(|(x, y)| at(&document, *x, *y) != Color::WHITE)
            .collect();
        assert_eq!(painted.len(), 8, "45° 1px 斜线应只有 8 个像素");
    }

    #[test]
    fn painting_after_moving_a_layer_lands_under_the_cursor() {
        let mut document = document();
        let mut history = History::new();
        let id = document.active_layer().unwrap().id;
        // 图层右移 10：左边 10 格在文档里空出来。
        document.set_layer_position(id, crate::document::Point::new(10, 0));

        // 直接在空出来的文档坐标 (2, 2) 落笔。
        let mut brush = BrushTool::paint().with_color(Color::RED);
        brush.on_pointer_down(
            &mut context(&mut document, &mut history),
            left(Vec2::new(2.0, 2.0)),
        );
        brush.on_pointer_up(
            &mut context(&mut document, &mut history),
            left(Vec2::new(2.0, 2.0)),
        );

        let layer = document.active_layer().unwrap();
        assert_eq!(
            layer.position,
            crate::document::Point::ZERO,
            "落笔前烘进像素"
        );
        assert_eq!(
            layer.pixels.get_pixel(2, 2),
            Color::RED,
            "笔迹在文档坐标下落笔"
        );
        assert_eq!(
            layer.pixels.get_pixel(12, 2),
            Color::WHITE,
            "不会偏移到 +10"
        );
    }

    #[test]
    fn undo_after_moving_the_layer_still_targets_the_moved_stroke() {
        let mut document = document();
        let mut history = History::new();
        let id = document.active_layer().unwrap().id;
        let mut brush = BrushTool::paint().with_color(Color::RED);
        // 笔画 A 在文档 (4, 4)。
        brush.on_pointer_down(
            &mut context(&mut document, &mut history),
            left(Vec2::new(4.0, 4.0)),
        );
        brush.on_pointer_up(
            &mut context(&mut document, &mut history),
            left(Vec2::new(4.0, 4.0)),
        );
        // 图层右移 10，A 的像素跟着去缓冲区索引 14。
        document.set_layer_position(id, crate::document::Point::new(10, 0));
        // 笔画 B 触发缓冲区扩展 + 历史区域平移。
        brush.on_pointer_down(
            &mut context(&mut document, &mut history),
            left(Vec2::new(20.0, 20.0)),
        );
        brush.on_pointer_up(
            &mut context(&mut document, &mut history),
            left(Vec2::new(20.0, 20.0)),
        );

        assert_eq!(history.undo(&mut document), Some("画笔"), "先撤 B");
        assert_eq!(history.undo(&mut document), Some("画笔"), "再撤 A");
        let layer = document.active_layer().unwrap();
        assert_eq!(layer.position, crate::document::Point::ZERO);
        assert_eq!(
            layer.pixels.get_pixel(14, 4),
            Color::WHITE,
            "撤销 A 应命中平移后的坐标"
        );
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
    fn a_clip_region_confines_the_stroke() {
        let mut document = document();
        let mut brush = BrushTool::paint().with_color(Color::RED);
        brush.clip = Some(PixelRegion::new(14, 14, 4, 4));
        brush.stroke_to(&mut document, Vec2::new(16.0, 16.0));
        assert_eq!(at(&document, 16, 16), Color::RED, "选区内落笔");
        assert_eq!(at(&document, 20, 16), Color::WHITE, "选区外不落笔");
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
        eprintln!("stroke (600px, size 1) {ms:.3} ms");
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
