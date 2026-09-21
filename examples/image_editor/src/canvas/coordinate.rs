//! 屏幕 -> 文档 -> 像素坐标转换。
//!
//! §12 要求所有坐标换算都走统一入口，UI / 工具代码里不允许自己算
//! `x / zoom`。相机提供浮点换算，这里再收口到像素下标。

use draw_core::Vec2;

use super::CanvasCamera;

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
}
