//! 画布：相机（缩放 / 平移）与坐标转换。
//!
//! 画布渲染走场景里的一个 `Node2D`：它的 `Transform2D` 就是
//! [`CanvasCamera::transform`]，`draw_scene` 在画它时套上这个变换，所以
//! **文档像素坐标 -> 屏幕逻辑坐标** 只有一个定义，相机的数学和节点的变换
//! 不会各算一套。这里只放纯数学，方便无头测试。

mod camera;
mod checkerboard;
mod coordinate;

pub use camera::CanvasCamera;
#[cfg(test)]
pub use checkerboard::color_at;
pub use checkerboard::paint_backdrop;
pub use coordinate::{document_to_pixel, pixel_selection, screen_to_document};

use draw_render::TextureId;

/// 文档合成结果在渲染后端里的纹理句柄。
///
/// 视图用它在文档 `Node2D` 的 `Visual::Image` 上引用纹理，宿主用同一个 id
/// 把合成结果注册进后端（`WgpuBackend::register_texture`）。约定一个固定值，
/// 两边不需要再互相传递句柄。
pub const DOCUMENT_TEXTURE: TextureId = TextureId::new(1);
