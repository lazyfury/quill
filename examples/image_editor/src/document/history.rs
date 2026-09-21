//! 撤销 / 重做：`Command` 模式 + 两条栈（[`History`]）+ 画笔命令。
//!
//! 约定：`Command::execute` 把命令的效果写回文档（首次应用与 redo 都走它），
//! `Command::undo` 撤回效果。命令记录的是**差异区域的前后像素**，不是整层
//! 快照 —— 一笔的撤销内存跟笔画覆盖的大小成正比。
//!
//! `History` 是**编辑会话**状态（不随文档保存 / 克隆），所以放在 `AppState`
//! 而不是 `Document` 里；命令本身属于文档领域，所以这个模块在 `document/`。
//!
//! Phase 6 只覆盖像素笔触：图层的增删 / 排序 / 不透明度暂时**不**入历史，
//! 等需要时再给它们各自的 `Command`。

use std::fmt;

use super::color::Color;
use super::document::Document;
use super::id::LayerId;
use super::layer::{BlendMode, Layer};
use super::pixel_buffer::PixelBuffer;
use super::point::Point;
use super::region::PixelRegion;

/// 一条可撤销的编辑。
pub trait Command: fmt::Debug {
    /// 首次应用或 redo：把效果写回 `document`。
    fn execute(&mut self, document: &mut Document);
    /// 撤销效果（恢复到 `execute` 之前）。
    fn undo(&mut self, document: &mut Document);
    /// 状态栏 / 日志里的短名字。
    fn label(&self) -> &'static str;
    /// 图层缓冲区重新对齐（内容索引整体平移 `(dx, dy)`，非负）时，同步平移
    /// 这条命令记录的区域。默认 no-op；只有记录像素坐标的命令需要实现。
    fn translate_for(&mut self, _layer: LayerId, _dx: i32, _dy: i32) {}
}

/// 撤销 / 重做栈。超过上限时丢弃最旧的命令，内存有界。
pub struct History {
    undo: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
    limit: usize,
}

impl History {
    /// 默认保留的撤销步数。
    pub const DEFAULT_LIMIT: usize = 32;

    pub fn new() -> Self {
        Self::with_limit(Self::DEFAULT_LIMIT)
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
        }
    }

    /// 执行一条**新**命令：应用它、压入 undo 栈、清空 redo 栈。
    ///
    /// 交互式编辑（画笔）在落笔时已经改过像素，这里会再把同样的像素写一遍；
    /// 命令写的是记录下来的字节，所以是幂等的。
    pub fn execute(&mut self, mut command: Box<dyn Command>, document: &mut Document) {
        command.execute(document);
        self.undo.push(command);
        self.redo.clear();
        self.trim();
    }

    /// 撤销一步，返回命令名字；没有可撤销的返回 `None`。
    pub fn undo(&mut self, document: &mut Document) -> Option<&'static str> {
        let mut command = self.undo.pop()?;
        command.undo(document);
        let label = command.label();
        self.redo.push(command);
        Some(label)
    }

    /// 重做一步，返回命令名字；没有可重做的返回 `None`。
    pub fn redo(&mut self, document: &mut Document) -> Option<&'static str> {
        let mut command = self.redo.pop()?;
        command.execute(document);
        let label = command.label();
        self.undo.push(command);
        self.trim();
        Some(label)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// 撤销栈里的命令名，从最旧到最新（历史面板显示用）。
    pub fn undo_labels(&self) -> Vec<&'static str> {
        self.undo.iter().map(|command| command.label()).collect()
    }

    /// 重做栈里的命令名，**下一个要重做的在最前**。
    pub fn redo_labels(&self) -> Vec<&'static str> {
        self.redo
            .iter()
            .rev()
            .map(|command| command.label())
            .collect()
    }

    /// 图层缓冲区重新对齐后，平移该图层上所有命令记录的区域，使它们在新的
    /// buffer 坐标系里仍指向同一块内容。`dx` / `dy` 非负。
    pub fn translate_layer(&mut self, layer: LayerId, dx: i32, dy: i32) {
        if dx == 0 && dy == 0 {
            return;
        }
        for command in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            command.translate_for(layer, dx, dy);
        }
    }

    fn trim(&mut self) {
        let excess = self.undo.len().saturating_sub(self.limit);
        if excess > 0 {
            self.undo.drain(0..excess);
        }
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

/// 克隆出来的历史是空的：撤销栈是编辑会话状态，不随文档复制。
impl Clone for History {
    fn clone(&self) -> Self {
        Self::with_limit(self.limit)
    }
}

impl PartialEq for History {
    fn eq(&self, other: &Self) -> bool {
        self.limit == other.limit
            && self.undo.len() == other.undo.len()
            && self.redo.len() == other.redo.len()
    }
}

impl fmt::Debug for History {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("History")
            .field("undo", &self.undo.len())
            .field("redo", &self.redo.len())
            .field("limit", &self.limit)
            .finish()
    }
}

/// 一次画笔 / 橡皮笔触：记录受影响区域的前后像素。
#[derive(Debug)]
pub struct PaintCommand {
    layer: LayerId,
    region: PixelRegion,
    before: Vec<Color>,
    after: Vec<Color>,
    label: &'static str,
}

impl PaintCommand {
    /// 从落笔前快照与当前图层缓冲抓出差异区域；没有像素变化时返回 `None`。
    pub fn capture(
        layer: LayerId,
        before: &PixelBuffer,
        after: &PixelBuffer,
        label: &'static str,
    ) -> Option<Self> {
        let region = PixelRegion::diff(before, after)?;
        Some(Self {
            layer,
            region,
            before: before.region(region),
            after: after.region(region),
            label,
        })
    }

    /// 把 `pixels` 写回目标图层；图层已被删掉时返回 `false`。
    fn write(&self, document: &mut Document, pixels: &[Color]) -> bool {
        match document.layer_mut(self.layer) {
            Some(layer) => {
                layer.pixels.put_region(self.region, pixels);
                true
            }
            None => false,
        }
    }
}

impl Command for PaintCommand {
    fn execute(&mut self, document: &mut Document) {
        if self.write(document, &self.after) {
            document.touch();
        }
    }

    fn undo(&mut self, document: &mut Document) {
        if self.write(document, &self.before) {
            document.touch();
        }
    }

    fn label(&self) -> &'static str {
        self.label
    }

    fn translate_for(&mut self, layer: LayerId, dx: i32, dy: i32) {
        if self.layer != layer {
            return;
        }
        self.region = self.region.translated(dx.max(0) as u32, dy.max(0) as u32);
    }
}

/// 一个图层的“非像素”状态。
#[derive(Debug, Clone, PartialEq)]
struct LayerMeta {
    id: LayerId,
    name: String,
    visible: bool,
    opacity: f32,
    blend_mode: BlendMode,
    position: Point,
}

/// 图层栈的非像素状态：顺序 + 每层元数据 + 当前图层。
///
/// 只存元数据，**不存像素**，所以移动 / 重命名 / 可见性 / 不透明度 / 排序
/// 这些操作的撤销代价极小。
#[derive(Debug, Clone, PartialEq)]
pub struct LayerStackMeta {
    layers: Vec<LayerMeta>,
    active: Option<LayerId>,
}

impl LayerStackMeta {
    fn capture(document: &Document) -> Self {
        Self {
            layers: document
                .layers
                .iter()
                .map(|layer| LayerMeta {
                    id: layer.id,
                    name: layer.name.clone(),
                    visible: layer.visible,
                    opacity: layer.opacity,
                    blend_mode: layer.blend_mode,
                    position: layer.position,
                })
                .collect(),
            active: document.active_layer,
        }
    }

    /// 把元数据 / 顺序 / 当前图层写回；像素不动（按 id 复用原 `Layer`）。
    fn restore(&self, document: &mut Document) {
        let mut current = std::mem::take(&mut document.layers);
        let mut restored = Vec::with_capacity(current.len());
        for meta in &self.layers {
            if let Some(index) = current.iter().position(|layer| layer.id == meta.id) {
                let mut layer = current.remove(index);
                layer.name.clone_from(&meta.name);
                layer.visible = meta.visible;
                layer.opacity = meta.opacity;
                layer.blend_mode = meta.blend_mode;
                layer.position = meta.position;
                restored.push(layer);
            }
        }
        restored.append(&mut current);
        document.layers = restored;
        document.active_layer = self.active;
        document.touch();
    }
}

/// 图层非像素状态的变更：移动 / 重命名 / 可见性 / 不透明度 / 排序。
#[derive(Debug)]
pub struct LayerMetaCommand {
    before: LayerStackMeta,
    after: LayerStackMeta,
    label: &'static str,
}

impl LayerMetaCommand {
    /// 改动前的快照（随后做改动，再 `new(before, after)`）。
    pub fn capture(document: &Document) -> LayerStackMeta {
        LayerStackMeta::capture(document)
    }

    pub fn new(before: LayerStackMeta, after: LayerStackMeta, label: &'static str) -> Self {
        Self {
            before,
            after,
            label,
        }
    }
}

impl Command for LayerMetaCommand {
    fn execute(&mut self, document: &mut Document) {
        self.after.restore(document);
    }

    fn undo(&mut self, document: &mut Document) {
        self.before.restore(document);
    }

    fn label(&self) -> &'static str {
        self.label
    }
}

/// 移动一个图层（只改 `position`）。
#[derive(Debug)]
pub struct SetLayerPositionCommand {
    layer: LayerId,
    before: Point,
    after: Point,
}

impl SetLayerPositionCommand {
    pub fn new(layer: LayerId, before: Point, after: Point) -> Self {
        Self {
            layer,
            before,
            after,
        }
    }
}

impl Command for SetLayerPositionCommand {
    fn execute(&mut self, document: &mut Document) {
        document.set_layer_position(self.layer, self.after);
    }

    fn undo(&mut self, document: &mut Document) {
        document.set_layer_position(self.layer, self.before);
    }

    fn label(&self) -> &'static str {
        "移动图层"
    }
}

/// 新建一个图层（带像素）；undo 时移除、redo 时插回。
#[derive(Debug)]
pub struct AddLayerCommand {
    id: LayerId,
    layer: Option<Layer>,
    index: usize,
    before_active: Option<LayerId>,
}

impl AddLayerCommand {
    pub fn new(layer: Layer, index: usize, before_active: Option<LayerId>) -> Self {
        Self {
            id: layer.id,
            layer: Some(layer),
            index,
            before_active,
        }
    }
}

impl Command for AddLayerCommand {
    fn execute(&mut self, document: &mut Document) {
        if let Some(layer) = self.layer.take() {
            let index = self.index.min(document.layers.len());
            document.layers.insert(index, layer);
            document.active_layer = Some(self.id);
            document.touch();
        }
    }

    fn undo(&mut self, document: &mut Document) {
        if let Some(index) = document.layers.iter().position(|layer| layer.id == self.id) {
            self.layer = Some(document.layers.remove(index));
            document.active_layer = self.before_active;
            document.touch();
        }
    }

    fn label(&self) -> &'static str {
        "新建图层"
    }
}

/// 删除一个图层；undo 时插回原位置与原来的当前图层。
#[derive(Debug)]
pub struct RemoveLayerCommand {
    id: LayerId,
    layer: Option<Layer>,
    index: usize,
    before_active: Option<LayerId>,
}

impl RemoveLayerCommand {
    pub fn new(id: LayerId, before_active: Option<LayerId>) -> Self {
        Self {
            id,
            layer: None,
            index: 0,
            before_active,
        }
    }
}

impl Command for RemoveLayerCommand {
    fn execute(&mut self, document: &mut Document) {
        if let Some(index) = document.layer_index(self.id) {
            self.index = index;
            self.layer = document.remove_layer(self.id);
        }
    }

    fn undo(&mut self, document: &mut Document) {
        if let Some(layer) = self.layer.take() {
            let index = self.index.min(document.layers.len());
            document.layers.insert(index, layer);
            document.active_layer = self.before_active;
            document.touch();
        }
    }

    fn label(&self) -> &'static str {
        "删除图层"
    }
}

/// 把图层裁到文档大小；undo 时恢复裁剪前的像素与位置。
#[derive(Debug)]
pub struct CropLayerCommand {
    id: LayerId,
    before: PixelBuffer,
    position: Point,
}

impl CropLayerCommand {
    pub fn new(id: LayerId, before: PixelBuffer, position: Point) -> Self {
        Self {
            id,
            before,
            position,
        }
    }
}

impl Command for CropLayerCommand {
    fn execute(&mut self, document: &mut Document) {
        document.crop_layer_to_document(self.id);
    }

    fn undo(&mut self, document: &mut Document) {
        if let Some(layer) = document.layer_mut(self.id) {
            layer.pixels = self.before.clone();
            layer.position = self.position;
            document.touch();
        }
    }

    fn label(&self) -> &'static str {
        "裁到文档"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Color;

    fn document() -> Document {
        Document::new("d", 8, 8)
    }

    /// 直接写一个像素，构造一条命令（不经过画笔）。
    fn paint_command(document: &mut Document) -> PaintCommand {
        let id = document.active_layer.unwrap();
        let before = document.layer(id).unwrap().pixels.clone();
        // 来回切换颜色，好让同一个 helper 能在一次历史里反复造命令。
        let next = if before.get_pixel(4, 4) == Color::RED {
            Color::BLACK
        } else {
            Color::RED
        };
        document.layer_mut(id).unwrap().pixels.set_pixel(4, 4, next);
        let after = document.layer(id).unwrap().pixels.clone();
        PaintCommand::capture(id, &before, &after, "画笔").expect("有变化")
    }

    fn pixel(document: &Document, x: u32, y: u32) -> Color {
        document.active_layer().unwrap().pixels.get_pixel(x, y)
    }

    #[test]
    fn execute_records_and_clears_redo() {
        let mut document = document();
        let mut history = History::new();
        let command = paint_command(&mut document);
        history.execute(Box::new(command), &mut document);

        assert!(history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_len(), 1);
    }

    #[test]
    fn undo_restores_and_redo_reapplies() {
        let mut document = document();
        let mut history = History::new();
        let command = paint_command(&mut document);
        history.execute(Box::new(command), &mut document);
        assert_eq!(pixel(&document, 4, 4), Color::RED);

        assert_eq!(history.undo(&mut document), Some("画笔"));
        assert_eq!(pixel(&document, 4, 4), Color::WHITE);
        assert!(history.can_redo());

        assert_eq!(history.redo(&mut document), Some("画笔"));
        assert_eq!(pixel(&document, 4, 4), Color::RED);
        assert!(!history.can_redo());
    }

    #[test]
    fn undo_and_redo_return_none_on_empty_stacks() {
        let mut document = document();
        let mut history = History::new();
        assert_eq!(history.undo(&mut document), None);
        assert_eq!(history.redo(&mut document), None);
        assert!(!history.can_undo());
    }

    #[test]
    fn a_new_command_clears_the_redo_stack() {
        let mut document = document();
        let mut history = History::new();
        let first = paint_command(&mut document);
        history.execute(Box::new(first), &mut document);
        history.undo(&mut document);
        assert!(history.can_redo());

        let second = paint_command(&mut document);
        history.execute(Box::new(second), &mut document);
        assert!(!history.can_redo(), "新命令之后 redo 栈应清空");
        assert_eq!(history.undo_len(), 1);
    }

    #[test]
    fn the_stack_is_bounded_by_the_limit() {
        let mut document = document();
        let mut history = History::with_limit(2);
        for _ in 0..5 {
            let command = paint_command(&mut document);
            history.execute(Box::new(command), &mut document);
        }
        assert_eq!(history.undo_len(), 2);
    }

    #[test]
    fn undo_touches_the_document_revision() {
        let mut document = document();
        let mut history = History::new();
        let command = paint_command(&mut document);
        history.execute(Box::new(command), &mut document);
        let revision = document.revision();
        history.undo(&mut document);
        assert_eq!(document.revision(), revision + 1, "撤销要让视图重合成");
    }

    #[test]
    fn labels_track_the_undo_and_redo_stacks() {
        let mut document = document();
        let mut history = History::new();
        let command = paint_command(&mut document);
        history.execute(Box::new(command), &mut document);
        assert_eq!(history.undo_labels(), vec!["画笔"]);
        assert!(history.redo_labels().is_empty());

        history.undo(&mut document);
        assert!(history.undo_labels().is_empty());
        assert_eq!(history.redo_labels(), vec!["画笔"]);
    }

    #[test]
    fn layer_meta_command_restores_metadata_and_order() {
        let mut document = document();
        let bottom = document.layers[0].id;
        let top = document.add_layer("top");

        let before = LayerMetaCommand::capture(&document);
        document.set_layer_visible(bottom, false);
        document.rename_layer(top, "重命名");
        document.set_layer_opacity(top, 0.5);
        document.move_layer(bottom, usize::MAX);
        let after = LayerMetaCommand::capture(&document);

        let mut command = LayerMetaCommand::new(before, after, "图层元数据");
        command.undo(&mut document);
        assert!(document.layer(bottom).unwrap().visible);
        assert_eq!(document.layer(top).unwrap().name, "top");
        assert_eq!(document.layer(top).unwrap().opacity, 1.0);
        assert_eq!(document.layers[0].id, bottom, "顺序复原");

        command.execute(&mut document);
        assert!(!document.layer(bottom).unwrap().visible);
        assert_eq!(document.layer(top).unwrap().name, "重命名");
        assert_eq!(document.layers.last().unwrap().id, bottom);
    }

    #[test]
    fn add_and_remove_layer_commands_round_trip() {
        let mut document = document();
        let before_active = document.active_layer;
        let layer = Layer::new("L", PixelBuffer::filled(2, 2, Color::RED));
        let id = layer.id;

        let mut add = AddLayerCommand::new(layer, document.layers.len(), before_active);
        add.execute(&mut document);
        assert!(document.layer(id).is_some());
        assert_eq!(document.active_layer, Some(id));
        add.undo(&mut document);
        assert!(document.layer(id).is_none());
        add.execute(&mut document);
        assert!(document.layer(id).is_some());

        let mut remove = RemoveLayerCommand::new(id, Some(id));
        remove.execute(&mut document);
        assert!(document.layer(id).is_none());
        remove.undo(&mut document);
        assert!(document.layer(id).is_some(), "撤销删除把图层插回");
    }
}
