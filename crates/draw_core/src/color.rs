/// An RGBA color with components in `0.0..=1.0`.
///
/// Values are interpreted in the working color space of the backend; the core
/// does not perform color management. Alpha is straight (non-premultiplied).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const RED: Self = Self::new(1.0, 0.0, 0.0, 1.0);
    pub const GREEN: Self = Self::new(0.0, 1.0, 0.0, 1.0);
    pub const BLUE: Self = Self::new(0.0, 0.0, 1.0, 1.0);
    /// Debug/selection yellow, used by component debug drawing.
    pub const YELLOW: Self = Self::new(1.0, 0.85, 0.10, 1.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    /// Builds a color from 8-bit channels.
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        const INV: f32 = 1.0 / 255.0;
        Self::new(
            r as f32 * INV,
            g as f32 * INV,
            b as f32 * INV,
            a as f32 * INV,
        )
    }

    /// Converts to 8-bit channels, clamping to `0.0..=1.0`.
    pub fn to_rgba8(self) -> [u8; 4] {
        fn channel(v: f32) -> u8 {
            (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
        }
        [
            channel(self.r),
            channel(self.g),
            channel(self.b),
            channel(self.a),
        ]
    }

    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Multiplies the existing alpha by `factor`.
    pub const fn alpha_mul(self, factor: f32) -> Self {
        Self {
            a: self.a * factor,
            ..self
        }
    }

    pub fn is_opaque(self) -> bool {
        self.a >= 1.0
    }

    pub fn is_transparent(self) -> bool {
        self.a <= 0.0
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
            self.a + (other.a - self.a) * t,
        )
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn from_array(v: [f32; 4]) -> Self {
        Self::new(v[0], v[1], v[2], v[3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    #[test]
    fn alpha_helpers() {
        assert!(Color::WHITE.is_opaque());
        assert!(!Color::WHITE.with_alpha(0.5).is_opaque());
        assert!(Color::TRANSPARENT.is_transparent());
        let c = Color::WHITE.alpha_mul(0.25);
        assert!((c.a - 0.25).abs() < EPS);
    }

    #[test]
    fn rgba8_round_trip() {
        let c = Color::from_rgba8(255, 128, 0, 64);
        assert_eq!(c.to_rgba8(), [255, 128, 0, 64]);
        assert_eq!(Color::RED.to_rgba8(), [255, 0, 0, 255]);
        // clamps out-of-range input
        assert_eq!(
            Color::new(2.0, -1.0, 0.5, 1.0).to_rgba8(),
            [255, 0, 128, 255]
        );
    }

    #[test]
    fn lerp() {
        let c = Color::BLACK.lerp(Color::WHITE, 0.5);
        assert!((c.r - 0.5).abs() < EPS);
        assert!((c.a - 1.0).abs() < EPS);
    }
}
