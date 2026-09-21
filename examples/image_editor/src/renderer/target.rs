//! 合成目标。现在就是一块 CPU 像素缓冲区；GPU 目标以后另加实现。

use crate::document::PixelBuffer;

/// 一次合成的输出。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderTarget {
    pub pixels: PixelBuffer,
}

impl RenderTarget {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            pixels: PixelBuffer::new(width, height),
        }
    }

    /// 保证目标尺寸与文档一致（尺寸变了就重新分配，内容无所谓，马上会被
    /// 合成覆盖）。
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.pixels.width != width || self.pixels.height != height {
            self.pixels = PixelBuffer::new(width, height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_reallocates_only_on_a_size_change() {
        let mut target = RenderTarget::new(2, 2);
        target.pixels.set_pixel(0, 0, crate::document::Color::RED);
        target.resize(2, 2);
        assert_eq!(
            target.pixels.get_pixel(0, 0),
            crate::document::Color::RED,
            "同尺寸不丢内容"
        );
        target.resize(3, 3);
        assert_eq!((target.pixels.width, target.pixels.height), (3, 3));
    }
}
