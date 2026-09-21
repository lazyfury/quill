//! 整数像素矩形：差异区域（撤销命令 / 以后的脏矩形）的单位。
//!
//! 坐标是文档 / 图层像素；`width` 或 `height` 为 0 就是空区域。越界不做假设，
//! 由使用方按 `PixelBuffer` 的尺寸裁剪。

use super::pixel_buffer::PixelBuffer;

/// 一个轴对齐的像素矩形，`x` / `y` 是左上角，宽高是**像素个数**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PixelRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRegion {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 右边界（不含）。
    pub const fn right(self) -> u32 {
        self.x + self.width
    }

    /// 下边界（不含）。
    pub const fn bottom(self) -> u32 {
        self.y + self.height
    }

    pub const fn area(self) -> usize {
        self.width as usize * self.height as usize
    }

    /// 像素是否落在区域内（框选裁剪切像素用）。
    pub const fn contains(self, x: u32, y: u32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }

    /// 整体平移 `(dx, dy)`（非负：只会在左上补空间时用到）。
    pub const fn translated(self, dx: u32, dy: u32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            width: self.width,
            height: self.height,
        }
    }

    /// 两张**同尺寸**缓冲之间的差异包围盒；完全相同或尺寸不同返回 `None`。
    ///
    /// 逐 4 字节比较原始数据，只有真正不同的像素才换算坐标，所以一笔的
    /// 撤销区域跟笔画大小成正比，跟画布大小无关（比较仍是整块扫描）。
    pub fn diff(before: &PixelBuffer, after: &PixelBuffer) -> Option<Self> {
        if before.width != after.width || before.height != after.height || before.width == 0 {
            return None;
        }
        let (mut min_x, mut min_y) = (u32::MAX, u32::MAX);
        let (mut max_x, mut max_y) = (0u32, 0u32);
        let mut changed = false;
        for (index, (a, b)) in before
            .data
            .chunks_exact(4)
            .zip(after.data.chunks_exact(4))
            .enumerate()
        {
            if a == b {
                continue;
            }
            let index = index as u32;
            let (x, y) = (index % before.width, index / before.width);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            changed = true;
        }
        changed.then(|| Self::new(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Color;

    #[test]
    fn a_region_reports_its_area_and_edges() {
        assert_eq!(PixelRegion::new(2, 3, 4, 5).area(), 20);
        assert_eq!(PixelRegion::new(2, 3, 4, 5).right(), 6);
        assert_eq!(PixelRegion::new(2, 3, 4, 5).bottom(), 8);
    }

    #[test]
    fn contains_is_half_open_on_the_right_and_bottom() {
        let region = PixelRegion::new(2, 3, 4, 5);
        assert!(region.contains(2, 3));
        assert!(region.contains(5, 7));
        assert!(!region.contains(6, 3), "右边界不含");
        assert!(!region.contains(2, 8), "下边界不含");
        assert!(!region.contains(1, 3));
    }

    #[test]
    fn diff_is_the_bounding_box_of_changed_pixels() {
        let before = PixelBuffer::filled(8, 8, Color::WHITE);
        let mut after = before.clone();
        after.set_pixel(2, 3, Color::BLACK);
        after.set_pixel(5, 6, Color::BLACK);
        // 包围盒覆盖两个点：(2,3)..=(5,6)。
        assert_eq!(
            PixelRegion::diff(&before, &after),
            Some(PixelRegion::new(2, 3, 4, 4))
        );
    }

    #[test]
    fn diff_of_identical_buffers_is_none() {
        let buffer = PixelBuffer::filled(4, 4, Color::WHITE);
        assert_eq!(PixelRegion::diff(&buffer, &buffer.clone()), None);
    }

    #[test]
    fn diff_of_mismatched_sizes_is_none() {
        let a = PixelBuffer::new(4, 4);
        let mut b = PixelBuffer::new(5, 4);
        b.set_pixel(0, 0, Color::RED);
        assert_eq!(PixelRegion::diff(&a, &b), None);
    }
}
