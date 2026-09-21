//! 文档：尺寸、图层栈和当前选中图层。
//!
//! 图层数组从下到上（`layers[0]` 是最底层），跟 §13 的合成顺序一致。
//!
//! `active_layer` 用 `Option`：新建文档会带一个"背景"图层，但删到空是合法的，
//! 用 `None` 表达"没有选中图层"，比留一个悬空 `LayerId` 更安全。
//!
//! 还没有的字段（故意留到后续 Phase）：
//!
//! - `dirty_region`：§26 的预留扩展点，等脏矩形渲染时再加。
//!
//! 撤销历史（Phase 6）**不**在这里：它是编辑会话状态，存在
//! [`crate::app::state::AppState`]；命令定义在 [`super::history`]。

use super::color::Color;
use super::id::{DocumentId, LayerId};
use super::layer::{clamp_opacity, Layer};
use super::pixel_buffer::PixelBuffer;
use super::point::Point;

/// 新建文档的默认尺寸（§29 Phase 2 的验收标准）。
pub const DEFAULT_WIDTH: u32 = 800;
pub const DEFAULT_HEIGHT: u32 = 600;

/// 一个图像文档。
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub id: DocumentId,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// 从下到上。
    pub layers: Vec<Layer>,
    pub active_layer: Option<LayerId>,
    pub background: Color,
    pub dirty: bool,
    /// 每次会改动文档的操作 +1；视图用它判断要不要重合成 / 刷新图层列表。
    revision: u64,
}

impl Document {
    /// 一个 800×600（或给定尺寸）、带白色背景图层的文档。
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self::with_background(name, width, height, Color::WHITE)
    }

    /// 带指定背景色、且背景图层已填充该色的文档。
    pub fn with_background(
        name: impl Into<String>,
        width: u32,
        height: u32,
        background: Color,
    ) -> Self {
        let layer = Layer::new("背景", PixelBuffer::filled(width, height, background));
        let id = layer.id;
        Self {
            id: DocumentId::next(),
            name: name.into(),
            width,
            height,
            layers: vec![layer],
            active_layer: Some(id),
            background,
            dirty: false,
            revision: 0,
        }
    }

    /// 变更计数（从 0 开始，每次改动 +1）。
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// 标记文档已改动：置脏并让变更计数前进。命令（[`super::history`]）在
    /// 写回像素后也调它，视图据此重合成。
    pub(crate) fn touch(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }
}

/// 图层查询 / 增删改排序：Phase 4 的图层面板与 Phase 6 的 history 命令消费；
/// 本阶段先由单元测试把栈序、选中回落等语义钉住。
#[allow(dead_code)]
impl Document {
    /// 没有图层的空文档（测试和"新建透明文档"用）。
    pub fn empty(name: impl Into<String>, width: u32, height: u32, background: Color) -> Self {
        Self {
            id: DocumentId::next(),
            name: name.into(),
            width,
            height,
            layers: Vec::new(),
            active_layer: None,
            background,
            dirty: false,
            revision: 0,
        }
    }

    // -- 查询 ------------------------------------------------------------

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|layer| layer.id == id)
    }

    /// 图层在栈里的下标（`0` = 最底层）。
    pub fn layer_index(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|layer| layer.id == id)
    }

    pub fn active_layer(&self) -> Option<&Layer> {
        self.active_layer.and_then(|id| self.layer(id))
    }

    pub fn active_layer_mut(&mut self) -> Option<&mut Layer> {
        let id = self.active_layer?;
        self.layer_mut(id)
    }

    // -- 变更 ------------------------------------------------------------

    /// 在最上面加一个全透明图层，并设为当前图层。
    pub fn add_layer(&mut self, name: impl Into<String>) -> LayerId {
        self.add_layer_with_pixels(name, PixelBuffer::new(self.width, self.height))
    }

    /// 在最上面加一个带指定像素的图层，并设为当前图层。
    ///
    /// 像素尺寸不要求等于文档（导入的图片可以更大/更小，合成时按 `position`
    /// 裁剪）；Phase 7 的导入用它把解码结果放进新图层。
    pub fn add_layer_with_pixels(
        &mut self,
        name: impl Into<String>,
        pixels: PixelBuffer,
    ) -> LayerId {
        let layer = Layer::new(name, pixels);
        let id = layer.id;
        self.layers.push(layer);
        self.active_layer = Some(id);
        self.touch();
        id
    }

    /// 删除一个图层，返回被删掉的图层。删掉的是当前图层时，选中相邻的一个。
    pub fn remove_layer(&mut self, id: LayerId) -> Option<Layer> {
        let index = self.layer_index(id)?;
        let removed = self.layers.remove(index);
        if self.active_layer == Some(id) {
            let next = index.min(self.layers.len().saturating_sub(1));
            self.active_layer = self.layers.get(next).map(|layer| layer.id);
        }
        self.touch();
        Some(removed)
    }

    pub fn rename_layer(&mut self, id: LayerId, name: impl Into<String>) -> bool {
        let Some(index) = self.layer_index(id) else {
            return false;
        };
        self.layers[index].name = name.into();
        self.touch();
        true
    }

    pub fn set_layer_visible(&mut self, id: LayerId, visible: bool) -> bool {
        let Some(index) = self.layer_index(id) else {
            return false;
        };
        self.layers[index].visible = visible;
        self.touch();
        true
    }

    /// 设置不透明度，钳到 `0.0..=1.0`。
    pub fn set_layer_opacity(&mut self, id: LayerId, opacity: f32) -> bool {
        let Some(index) = self.layer_index(id) else {
            return false;
        };
        self.layers[index].opacity = clamp_opacity(opacity);
        self.touch();
        true
    }

    /// 设置图层相对文档原点的像素偏移（Phase 8 的移动工具）。
    /// 位置没变时返回 `false`，不前进 `revision`。
    pub fn set_layer_position(&mut self, id: LayerId, position: Point) -> bool {
        let Some(index) = self.layer_index(id) else {
            return false;
        };
        if self.layers[index].position == position {
            return false;
        }
        self.layers[index].position = position;
        self.touch();
        true
    }

    /// 把图层移到 `new_index`（超出范围时夹到末尾）。
    pub fn move_layer(&mut self, id: LayerId, new_index: usize) -> bool {
        let Some(old_index) = self.layer_index(id) else {
            return false;
        };
        let layer = self.layers.remove(old_index);
        let destination = new_index.min(self.layers.len());
        self.layers.insert(destination, layer);
        self.touch();
        true
    }

    /// 选中一个存在的图层。
    pub fn select_layer(&mut self, id: LayerId) -> bool {
        if self.layer(id).is_some() {
            self.active_layer = Some(id);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_document_is_the_default_size_with_a_background_layer() {
        let document = Document::new("测试", DEFAULT_WIDTH, DEFAULT_HEIGHT);
        assert_eq!((document.width, document.height), (800, 600));
        assert_eq!(document.name, "测试");
        assert_eq!(document.layers.len(), 1);
        assert_eq!(document.background, Color::WHITE);
        assert!(!document.dirty);

        let background = document.active_layer().expect("一个背景图层");
        assert_eq!(background.name, "背景");
        assert!(background.visible);
        assert_eq!(background.pixels.get_pixel(0, 0), Color::WHITE);
    }

    #[test]
    fn add_layer_stacks_on_top_and_selects_it() {
        let mut document = Document::new("d", 4, 3);
        let background = document.layers[0].id;
        let added = document.add_layer("Layer 1");

        assert_eq!(document.layers.len(), 2);
        assert_eq!(document.layers.last().unwrap().id, added, "加在最上面");
        assert_eq!(document.active_layer, Some(added));
        assert!(document.dirty);
        // 新图层是全透明的，尺寸跟文档一致。
        let layer = document.layer(added).unwrap();
        assert_eq!(layer.pixels.get_pixel(0, 0), Color::TRANSPARENT);
        assert_eq!((layer.pixels.width, layer.pixels.height), (4, 3));
        assert_ne!(added, background);
    }

    #[test]
    fn add_layer_with_pixels_keeps_the_given_size_and_content() {
        let mut document = Document::new("d", 4, 3);
        let pixels = PixelBuffer::filled(2, 5, Color::RED);
        let id = document.add_layer_with_pixels("导入", pixels);

        let layer = document.layer(id).expect("imported layer");
        assert_eq!((layer.pixels.width, layer.pixels.height), (2, 5));
        assert_eq!(layer.pixels.get_pixel(0, 0), Color::RED);
        assert_eq!(document.active_layer, Some(id));
    }

    #[test]
    fn removing_the_active_layer_selects_a_neighbour() {
        let mut document = Document::new("d", 2, 2);
        let bottom = document.layers[0].id;
        let top = document.add_layer("top");
        assert_eq!(document.active_layer, Some(top));

        let removed = document.remove_layer(top).expect("top 存在");
        assert_eq!(removed.id, top);
        assert_eq!(document.active_layer, Some(bottom), "回落到下面的图层");
    }

    #[test]
    fn removing_the_last_layer_leaves_no_active_layer() {
        let mut document = Document::new("d", 2, 2);
        let only = document.layers[0].id;
        assert!(document.remove_layer(only).is_some());
        assert!(document.layers.is_empty());
        assert_eq!(document.active_layer, None);
        assert!(document.active_layer().is_none());
        // 删不存在的图层返回 None，不改状态。
        assert!(document.remove_layer(only).is_none());
    }

    #[test]
    fn rename_select_and_visibility_only_touch_existing_layers() {
        let mut document = Document::new("d", 2, 2);
        let bottom = document.layers[0].id;
        let top = document.add_layer("top");

        assert!(document.rename_layer(top, "上层"));
        assert_eq!(document.layer(top).unwrap().name, "上层");

        assert!(document.select_layer(bottom));
        assert_eq!(document.active_layer, Some(bottom));

        assert!(document.set_layer_visible(bottom, false));
        assert!(!document.layer(bottom).unwrap().visible);

        // 不存在的 id 一律 false，且不影响选中。
        let ghost = Layer::new("ghost", PixelBuffer::new(1, 1)).id;
        assert!(!document.rename_layer(ghost, "x"));
        assert!(!document.set_layer_visible(ghost, true));
        assert!(!document.set_layer_opacity(ghost, 0.5));
        assert!(!document.select_layer(ghost));
        assert_eq!(document.active_layer, Some(bottom));
    }

    #[test]
    fn opacity_is_clamped_through_the_document() {
        let mut document = Document::new("d", 2, 2);
        let id = document.layers[0].id;
        assert!(document.set_layer_opacity(id, 2.0));
        assert_eq!(document.layer(id).unwrap().opacity, 1.0);
        assert!(document.set_layer_opacity(id, -1.0));
        assert_eq!(document.layer(id).unwrap().opacity, 0.0);
    }

    #[test]
    fn move_layer_reorders_bottom_and_top() {
        let mut document = Document::new("d", 1, 1);
        let bottom = document.layers[0].id;
        let middle = document.add_layer("middle");
        let top = document.add_layer("top");
        assert_eq!(document.layers[0].id, bottom);

        // 把最底层移到最上面。
        assert!(document.move_layer(bottom, usize::MAX));
        assert_eq!(document.layers.last().unwrap().id, bottom);
        assert_eq!(document.layers[0].id, middle);
        assert_eq!(document.layers[1].id, top);

        // 再把底移回最底。
        assert!(document.move_layer(bottom, 0));
        assert_eq!(document.layers[0].id, bottom);
        assert_eq!(document.layers[2].id, top);
    }

    #[test]
    fn mutations_advance_the_revision_but_selection_does_not() {
        let mut document = Document::new("d", 2, 2);
        let start = document.revision();
        let id = document.add_layer("L");
        assert_eq!(document.revision(), start + 1);
        assert!(document.set_layer_visible(id, false));
        assert_eq!(document.revision(), start + 2);
        assert!(document.select_layer(document.layers[0].id));
        assert_eq!(document.revision(), start + 2, "选中不是像素改动");
    }
}
