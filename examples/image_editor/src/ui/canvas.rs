//! 画布区域。
//!
//! 这里**不画任何东西**：文档由场景里的一个 `Node2D`（`Visual::Image`）在
//! UI 之下绘制。这一段只是一块透明区域，用来给画布定位（fit）和做命中测试
//! （滚轮缩放 / 中键平移）。用 `MouseFilter::Ignore` 把它从 UI 命中里摘出去，
//! 否则那块矩形会把指针事件全部吃掉。

use draw_components::{Component, Flex};
use draw_ui::MouseFilter;

/// 一块透明的画布区域。
pub fn canvas_area() -> impl Component {
    Flex::new().grow(1.0).mouse_filter(MouseFilter::Ignore)
}
