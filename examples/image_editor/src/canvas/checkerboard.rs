//! 画布的透明棋盘格背景。
//!
//! 只在**显示**用的合成结果上生成（[`paint_backdrop`] 由
//! `EditorView::take_texture_upload` 调用）：导出的 PNG 仍然走
//! `renderer::Renderer`，是带 alpha 的原始合成，所以棋盘格不会混进导出文件。
//!
//! 棋盘格按**文档像素坐标**对齐，跟着文档节点的相机变换一起缩放，因此文档里
//! 隐藏「背景」图层、或擦出透明区域时，露出的就是这张棋盘格。

use crate::document::{blend_over, Color, PixelBuffer};

/// 格子边长（文档像素）。
pub const CELL: u32 = 8;

/// 亮格颜色。
pub const LIGHT: Color = Color::rgb(255, 255, 255);
/// 暗格颜色。
pub const DARK: Color = Color::rgb(204, 204, 204);

/// `(x, y)` 处的棋盘格颜色（左上角为亮格）。
pub fn color_at(x: u32, y: u32) -> Color {
    if (x / CELL + y / CELL) % 2 == 0 {
        LIGHT
    } else {
        DARK
    }
}

/// 把 `pixels` 里的透明 / 半透明区域补上棋盘格：棋盘格在下，原像素朝上叠。
///
/// 不透明像素原样保留（快路，笔刷重合成时大部分像素走这里）；透明像素直接
/// 变成棋盘格；半透明像素与棋盘格做 source-over，显示时不会露出未定义颜色。
pub fn paint_backdrop(pixels: &mut PixelBuffer) {
    let width = pixels.width as usize;
    if width == 0 {
        return;
    }
    for (index, dst) in pixels.data.chunks_exact_mut(4).enumerate() {
        if dst[3] == 255 {
            continue;
        }
        let x = (index % width) as u32;
        let y = (index / width) as u32;
        let mut out = color_at(x, y).to_rgba8();
        blend_over(&mut out, dst, 1.0);
        dst.copy_from_slice(&out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_alternate_across_and_down() {
        assert_eq!(color_at(0, 0), LIGHT);
        assert_eq!(color_at(CELL, 0), DARK);
        assert_eq!(color_at(0, CELL), DARK);
        assert_eq!(color_at(CELL, CELL), LIGHT);
    }

    #[test]
    fn opaque_pixels_are_left_alone() {
        let mut pixels = PixelBuffer::filled(CELL, CELL, Color::RED);
        paint_backdrop(&mut pixels);
        assert_eq!(pixels.get_pixel(0, 0), Color::RED);
        assert_eq!(pixels.get_pixel(CELL - 1, CELL - 1), Color::RED);
    }

    #[test]
    fn transparent_pixels_become_the_checkerboard() {
        let mut pixels = PixelBuffer::new(CELL * 2, CELL);
        paint_backdrop(&mut pixels);
        assert_eq!(pixels.get_pixel(0, 0), LIGHT);
        assert_eq!(pixels.get_pixel(CELL, 0), DARK);
    }

    #[test]
    fn a_semi_transparent_pixel_is_composited_over_the_checkerboard() {
        let mut pixels = PixelBuffer::new(CELL, CELL);
        pixels.set_pixel(0, 0, Color::BLACK.with_alpha(128));
        paint_backdrop(&mut pixels);
        let color = pixels.get_pixel(0, 0);
        assert_eq!(color.a, 255, "补完背景后应不透明");
        assert!(
            (color.r as i32 - 127).abs() <= 1,
            "黑 50% 叠白格应为中灰，得到 {}",
            color.r
        );
    }
}
