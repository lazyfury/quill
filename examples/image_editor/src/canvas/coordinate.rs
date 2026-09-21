//! 屏幕 -> 文档 -> 像素坐标转换。
//!
//! §12 要求所有坐标换算都走统一入口，UI / 工具代码里不允许自己算
//! `x / zoom`。相机提供浮点换算，这里再收口到像素下标。

use draw_core::Vec2;

use super::CanvasCamera;
use crate::document::PixelRegion;

/// 屏幕逻辑坐标 -> 文档坐标（浮点，可能落在文档外）。
pub fn screen_to_document(camera: CanvasCamera, screen: Vec2) -> Vec2 {
    camera.screen_to_document(screen)
}

/// 文档坐标 -> 像素下标；落在 `[0, width) × [0, height)` 之外返回 `None`。
///
/// 非有限值（`NaN` / `inf`）返回 `None`，不会因为浮点转整数而悄悄落到 `(0,0)`。
pub fn document_to_pixel(document: Vec2, width: u32, height: u32) -> Option<(u32, u32)> {
    if !document.x.is_finite() || !document.y.is_finite() {
        return None;
    }
    if document.x < 0.0 || document.y < 0.0 {
        return None;
    }
    let x = document.x.floor();
    let y = document.y.floor();
    if x >= width as f32 || y >= height as f32 {
        return None;
    }
    Some((x as u32, y as u32))
}

/// 两个文档坐标角点（框选拖拽的起点 / 当前点）-> 裁剪到画布内的整数像素选区。
///
/// `None` 表示空选区：面积为 0、完全落在画布外，或坐标非有限。选区永远被夹在
/// `0..width × 0..height` 内，所以画笔 / 命令拿到的区域不会越界。
pub fn pixel_selection(
    anchor: Vec2,
    current: Vec2,
    width: u32,
    height: u32,
) -> Option<PixelRegion> {
    if !(anchor.x.is_finite()
        && anchor.y.is_finite()
        && current.x.is_finite()
        && current.y.is_finite())
    {
        return None;
    }
    let left = anchor.x.min(current.x).floor().max(0.0);
    let top = anchor.y.min(current.y).floor().max(0.0);
    let right = anchor.x.max(current.x).ceil().min(width as f32);
    let bottom = anchor.y.max(current.y).ceil().min(height as f32);
    if right <= left || bottom <= top {
        return None;
    }
    Some(PixelRegion::new(
        left as u32,
        top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_point_maps_through_the_camera() {
        let camera = CanvasCamera::new(2.0, Vec2::new(10.0, 20.0));
        // (30, 60) -> ((30-10)/2, (60-20)/2) = (10, 20)
        assert_eq!(
            screen_to_document(camera, Vec2::new(30.0, 60.0)),
            Vec2::new(10.0, 20.0)
        );
    }

    #[test]
    fn pixels_are_floored_and_bounds_checked() {
        assert_eq!(document_to_pixel(Vec2::new(3.9, 4.1), 10, 10), Some((3, 4)));
        // 边界外 / 负值 / 恰好等于尺寸。
        assert_eq!(document_to_pixel(Vec2::new(-0.1, 0.0), 10, 10), None);
        assert_eq!(document_to_pixel(Vec2::new(10.0, 0.0), 10, 10), None);
        assert_eq!(document_to_pixel(Vec2::new(0.0, 10.0), 10, 10), None);
    }

    #[test]
    fn non_finite_document_points_do_not_become_pixel_zero() {
        assert_eq!(document_to_pixel(Vec2::new(f32::NAN, 1.0), 10, 10), None);
        assert_eq!(
            document_to_pixel(Vec2::new(1.0, f32::INFINITY), 10, 10),
            None
        );
    }

    #[test]
    fn a_selection_is_ordered_clamped_and_half_open() {
        // 从右下往左上拖，顺序被归一化；小数向外取整。
        let region = pixel_selection(Vec2::new(8.4, 6.7), Vec2::new(2.2, 1.1), 10, 10);
        assert_eq!(region, Some(PixelRegion::new(2, 1, 7, 6)));

        // 超出画布的部分被夹住。
        let region = pixel_selection(Vec2::new(-5.0, -5.0), Vec2::new(4.0, 4.0), 10, 10);
        assert_eq!(region, Some(PixelRegion::new(0, 0, 4, 4)));

        // 空选区 / 完全在外 / 非有限。
        assert_eq!(
            pixel_selection(Vec2::new(3.0, 3.0), Vec2::new(3.0, 3.0), 10, 10),
            None
        );
        assert_eq!(
            pixel_selection(Vec2::new(-9.0, -9.0), Vec2::new(-1.0, -1.0), 10, 10),
            None
        );
        assert_eq!(
            pixel_selection(Vec2::new(f32::NAN, 0.0), Vec2::new(1.0, 1.0), 10, 10),
            None
        );
    }
}
