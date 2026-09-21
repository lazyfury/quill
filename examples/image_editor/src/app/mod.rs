//! 应用层：状态 + 窗口宿主。
//!
//! `state` 是纯数据（不依赖任何 UI 类型），`application` 是 winit + wgpu
//! 宿主。方向保持 `app -> ui -> 领域`，宿主不把平台类型泄漏进视图。

pub mod application;
pub mod state;
