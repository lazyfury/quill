//! The embedded WGSL shader used by the wgpu backend.

/// Source of the single textured-triangle pipeline.
pub const SHADER: &str = include_str!("shader.wgsl");
