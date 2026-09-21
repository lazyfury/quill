//! 图层：像素 + 可见性 / 透明度 / 混合模式 / 位置。

use super::id::LayerId;
use super::pixel_buffer::PixelBuffer;
use super::point::Point;

/// 图层混合模式。Phase 2 只有 `Normal`。
///
/// TODO(v1.1): 增加 `Multiply` / `Screen` / `Overlay` / `Darken` / `Lighten`
/// 等；合成在 Phase 4/GPU 阶段接进来，这里先把枚举形状定好，避免以后改
/// `Layer` 的公共字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    #[default]
    Normal,
}

/// 一个图层。像素尺寸由 `PixelBuffer` 决定，可以小于/大于文档。
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub visible: bool,
    /// 0.0..=1.0，写入时会被 [`Layer::set_opacity`] / [`Document`] 钳制。
    ///
    /// [`Document`]: super::Document
    pub opacity: f32,
    pub blend_mode: BlendMode,
    /// 图层像素缓冲区的原点：`pixels[0, 0]` 在文档坐标里的位置。
    ///
    /// 可以是负数、缓冲区也可以比文档大，所以移出画布的内容会留在缓冲里
    /// （可以再移回来），不会因为移动而丢掉。
    pub position: Point,
    pub pixels: PixelBuffer,
}

impl Layer {
    /// 一个可见、不透明、`Normal`、原点对齐的新图层。
    pub fn new(name: impl Into<String>, pixels: PixelBuffer) -> Self {
        Self {
            id: LayerId::next(),
            name: name.into(),
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            position: Point::ZERO,
            pixels,
        }
    }

    /// 设置不透明度，钳到 `0.0..=1.0`（非有限值按 1.0 处理）。
    #[allow(dead_code)] // 由 Phase 4 的图层不透明度控件消费。
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = clamp_opacity(opacity);
    }
}

/// 把任意浮点钳成合法的图层不透明度。
#[allow(dead_code)] // 只能通过图层不透明度入口到达，见上。
pub(crate) fn clamp_opacity(opacity: f32) -> f32 {
    if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_layer_defaults_are_sane() {
        let layer = Layer::new("Layer 1", PixelBuffer::new(4, 4));
        assert_eq!(layer.name, "Layer 1");
        assert!(layer.visible);
        assert_eq!(layer.opacity, 1.0);
        assert_eq!(layer.blend_mode, BlendMode::Normal);
        assert_eq!(layer.position, Point::ZERO);
    }

    #[test]
    fn set_opacity_clamps_and_rejects_non_finite() {
        let mut layer = Layer::new("L", PixelBuffer::new(1, 1));
        layer.set_opacity(0.25);
        assert_eq!(layer.opacity, 0.25);
        layer.set_opacity(-2.0);
        assert_eq!(layer.opacity, 0.0);
        layer.set_opacity(3.0);
        assert_eq!(layer.opacity, 1.0);
        layer.set_opacity(f32::NAN);
        assert_eq!(layer.opacity, 1.0);
    }
}
