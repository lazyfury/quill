//! 8-bit RGBA 像素颜色。
//!
//! 和 UI 层 `draw_core::Color`（f32、0..1）刻意分开：像素数据就是 8 位整数，
//! 两者之间在需要画色块时转换，而不是让像素模型带上浮点。

/// 一个 RGBA 像素颜色，每个通道 8 位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[allow(dead_code)] // 数据模型的公共表面；Phase 3/5 才消费，本阶段先由测试钉住语义。
impl Color {
    /// 全透明黑（`PixelBuffer` 清空后的默认值）。
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    pub const BLACK: Self = Self::new(0, 0, 0, 255);
    pub const WHITE: Self = Self::new(255, 255, 255, 255);
    pub const RED: Self = Self::new(255, 0, 0, 255);

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// 不透明 RGB。
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    pub const fn from_rgba8(rgba: [u8; 4]) -> Self {
        Self::new(rgba[0], rgba[1], rgba[2], rgba[3])
    }

    pub const fn to_rgba8(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// 同样的 RGB，换一个 alpha。
    pub const fn with_alpha(self, a: u8) -> Self {
        Self::new(self.r, self.g, self.b, a)
    }

    pub const fn is_transparent(self) -> bool {
        self.a == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consts_are_the_expected_rgba() {
        assert_eq!(Color::TRANSPARENT.to_rgba8(), [0, 0, 0, 0]);
        assert_eq!(Color::BLACK.to_rgba8(), [0, 0, 0, 255]);
        assert_eq!(Color::WHITE.to_rgba8(), [255, 255, 255, 255]);
        assert_eq!(Color::RED.to_rgba8(), [255, 0, 0, 255]);
    }

    #[test]
    fn rgba8_round_trips() {
        let color = Color::from_rgba8([12, 34, 56, 78]);
        assert_eq!(color.to_rgba8(), [12, 34, 56, 78]);
        assert_eq!(color, Color::new(12, 34, 56, 78));
    }

    #[test]
    fn with_alpha_keeps_rgb_and_flags_transparency() {
        let color = Color::WHITE.with_alpha(0);
        assert_eq!(color.to_rgba8(), [255, 255, 255, 0]);
        assert!(color.is_transparent());
        assert!(!Color::WHITE.is_transparent());
    }
}
