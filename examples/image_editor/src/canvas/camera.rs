//! 画布相机：文档像素 -> 屏幕逻辑坐标。

use draw_core::{Rect, Size, Transform2D, Vec2};

/// 相机的放大上限 / 下限。太小的缩放会让图像退化成一个点，太大则失去意义。
pub const MIN_ZOOM: f32 = 0.05;
pub const MAX_ZOOM: f32 = 64.0;

/// 画布相机。
///
/// 模型很简单：`offset` 是文档原点 `(0,0)` 在屏幕上的位置，`zoom` 是缩放。
/// 因此
///
/// ```text
/// screen = offset + document * zoom
/// document = (screen - offset) / zoom
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasCamera {
    pub zoom: f32,
    /// 文档原点在屏幕逻辑坐标里的位置。
    pub offset: Vec2,
}

impl Default for CanvasCamera {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: Vec2::ZERO,
        }
    }
}

impl CanvasCamera {
    /// 便捷构造（钳制缩放）。Phase 5 的工具与自检会用到。
    #[allow(dead_code)]
    pub fn new(zoom: f32, offset: Vec2) -> Self {
        Self {
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            offset,
        }
    }

    /// 画布变换（文档局部坐标 -> 屏幕坐标），就是这个 `Node2D` 的世界变换。
    pub fn transform(self) -> Transform2D {
        Transform2D::from_scale_rotation_origin(Vec2::splat(self.zoom), 0.0, self.offset)
    }

    /// 文档坐标 -> 屏幕坐标。渲染走 [`CanvasCamera::transform`]，这里是对外
    /// 的坐标 API（命中测试 / 工具会用到）。
    #[allow(dead_code)]
    pub fn document_to_screen(self, document: Vec2) -> Vec2 {
        self.offset + document * self.zoom
    }

    /// 屏幕坐标 -> 文档坐标（浮点；需要像素下标时再走
    /// [`document_to_pixel`](super::document_to_pixel)）。
    pub fn screen_to_document(self, screen: Vec2) -> Vec2 {
        (screen - self.offset) / self.zoom
    }

    /// 以 `screen` 为锚点缩放：锚点下的文档位置保持不动。
    pub fn zoom_at(&mut self, screen: Vec2, factor: f32) {
        let anchor = self.screen_to_document(screen);
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.zoom = zoom;
        self.offset = screen - anchor * zoom;
    }

    /// 按屏幕像素平移。
    pub fn pan_by(&mut self, delta: Vec2) {
        self.offset += delta;
    }

    /// 以给定缩放把文档居中放到 `area` 里。
    pub fn centered(area: Rect, document: Size, zoom: f32) -> Self {
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let area_vec = Vec2::new(area.size.width, area.size.height);
        let document_vec = Vec2::new(document.width, document.height) * zoom;
        Self {
            zoom,
            offset: area.origin + (area_vec - document_vec) * 0.5,
        }
    }

    /// 适配：取能放进 `area`（留 `padding`）的最大缩放，并居中。
    pub fn fit(area: Rect, document: Size, padding: f32) -> Self {
        let available = Vec2::new(
            (area.size.width - 2.0 * padding).max(1.0),
            (area.size.height - 2.0 * padding).max(1.0),
        );
        let document = Vec2::new(document.width.max(1.0), document.height.max(1.0));
        let zoom = (available.x / document.x).min(available.y / document.y);
        Self::centered(area, Size::new(document.x, document.y), zoom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_min_size(Vec2::new(100.0, 50.0), Size::new(400.0, 300.0))
    }

    #[test]
    fn default_is_identity() {
        let camera = CanvasCamera::default();
        assert_eq!(camera.transform(), Transform2D::IDENTITY);
        assert_eq!(
            camera.document_to_screen(Vec2::new(7.0, 9.0)),
            Vec2::new(7.0, 9.0)
        );
    }

    #[test]
    fn document_and_screen_round_trip() {
        let camera = CanvasCamera::new(2.0, Vec2::new(30.0, 40.0));
        let document = Vec2::new(5.0, 6.0);
        let screen = camera.document_to_screen(document);
        assert_eq!(screen, Vec2::new(40.0, 52.0));
        assert_eq!(camera.screen_to_document(screen), document);
    }

    #[test]
    fn the_transform_matches_the_math() {
        let camera = CanvasCamera::new(1.5, Vec2::new(10.0, -5.0));
        let point = Vec2::new(20.0, 30.0);
        assert_eq!(
            camera.transform().transform_point(point),
            camera.document_to_screen(point)
        );
    }

    #[test]
    fn zooming_keeps_the_anchor_fixed() {
        let mut camera = CanvasCamera::new(1.0, Vec2::ZERO);
        let anchor = Vec2::new(120.0, 80.0);
        let before = camera.screen_to_document(anchor);
        camera.zoom_at(anchor, 2.0);
        assert_eq!(camera.zoom, 2.0);
        assert_eq!(camera.screen_to_document(anchor), before);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut camera = CanvasCamera::default();
        camera.zoom_at(Vec2::ZERO, 1e9);
        assert_eq!(camera.zoom, MAX_ZOOM);
        camera.zoom_at(Vec2::ZERO, 1e-9);
        assert_eq!(camera.zoom, MIN_ZOOM);
    }

    #[test]
    fn pan_moves_the_origin() {
        let mut camera = CanvasCamera::default();
        camera.pan_by(Vec2::new(10.0, -4.0));
        assert_eq!(camera.offset, Vec2::new(10.0, -4.0));
    }

    #[test]
    fn fit_centers_the_document_in_the_area() {
        let camera = CanvasCamera::fit(area(), Size::new(200.0, 100.0), 20.0);
        // 可用 360×260，文档 200×100 -> zoom = min(1.8, 2.6) = 1.8
        assert!((camera.zoom - 1.8).abs() < 1e-4, "zoom = {}", camera.zoom);
        let center = camera.document_to_screen(Vec2::new(100.0, 50.0));
        assert!((center.x - 300.0).abs() < 1e-3, "center = {center:?}");
        assert!((center.y - 200.0).abs() < 1e-3, "center = {center:?}");
    }

    #[test]
    fn fit_to_a_tiny_area_still_has_a_usable_zoom() {
        let camera = CanvasCamera::fit(
            Rect::from_min_size(Vec2::ZERO, Size::new(4.0, 4.0)),
            Size::new(800.0, 600.0),
            16.0,
        );
        assert!(camera.zoom >= MIN_ZOOM);
        assert!(camera.zoom > 0.0);
    }
}
