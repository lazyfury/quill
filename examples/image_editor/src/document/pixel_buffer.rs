//! 统一的像素存储：行优先 RGBA。

use std::fmt;

use super::color::Color;
use super::region::PixelRegion;

/// 一块 RGBA 像素数据。每个像素 4 字节，`data.len() == width * height * 4`。
///
/// 所有越界访问都不 panic：读返回 [`Color::TRANSPARENT`]，写是 no-op。调用方
/// 是像素编辑器，越界是常态（笔刷扫过画布边缘），不该让进程崩。
#[derive(Clone, PartialEq, Eq)]
pub struct PixelBuffer {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl PixelBuffer {
    /// 一块全透明的缓冲区。
    pub fn new(width: u32, height: u32) -> Self {
        match byte_len(width, height) {
            Some(len) => Self {
                width,
                height,
                data: vec![0; len],
            },
            // `width * height * 4` 超过 `usize` 的尺寸根本无法分配；退化成
            // 空缓冲区，而不是 panic 或声称一个不存在的尺寸。
            None => Self {
                width: 0,
                height: 0,
                data: Vec::new(),
            },
        }
    }

    /// 一块填满 `color` 的缓冲区。
    pub fn filled(width: u32, height: u32, color: Color) -> Self {
        let mut buffer = Self::new(width, height);
        buffer.clear(color);
        buffer
    }
}

/// 像素访问 / 变形 API：Phase 3 的 Canvas 与 Phase 5 的 Brush 消费；
/// 本阶段先由单元测试把“越界不 panic”等语义钉住。
#[allow(dead_code)]
impl PixelBuffer {
    /// 从已有的 RGBA 字节构造；长度对不上时返回 `None`。
    pub fn from_rgba8(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        if data.len() == byte_len(width, height)? {
            Some(Self {
                width,
                height,
                data,
            })
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 像素坐标是否落在缓冲区内。
    pub fn contains(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height
    }

    /// 读取一个像素；越界返回 [`Color::TRANSPARENT`]。
    pub fn get_pixel(&self, x: u32, y: u32) -> Color {
        match self.index(x, y) {
            Some(index) => Color::from_rgba8([
                self.data[index],
                self.data[index + 1],
                self.data[index + 2],
                self.data[index + 3],
            ]),
            None => Color::TRANSPARENT,
        }
    }

    /// 写入一个像素；越界是 no-op。
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if let Some(index) = self.index(x, y) {
            self.data[index..index + 4].copy_from_slice(&color.to_rgba8());
        }
    }

    /// 按行优先读出一个矩形区域的像素（撤销命令记录前后像素用）。
    /// 越界的坐标读到 [`Color::TRANSPARENT`]。
    pub fn region(&self, region: PixelRegion) -> Vec<Color> {
        let mut out = Vec::with_capacity(region.area());
        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                out.push(self.get_pixel(x, y));
            }
        }
        out
    }

    /// 按行优先把一个矩形区域的像素写回（撤销命令恢复前后像素用）。
    /// `colors` 不足时多出的像素保持原样；越界像素跳过。
    pub fn put_region(&mut self, region: PixelRegion, colors: &[Color]) {
        for (index, y) in (region.y..region.bottom()).enumerate() {
            for (column, x) in (region.x..region.right()).enumerate() {
                let offset = index * region.width as usize + column;
                if let Some(color) = colors.get(offset) {
                    self.set_pixel(x, y, *color);
                }
            }
        }
    }

    /// 把 `source` 以直通 alpha 的 `source-over` 混到像素上，`source.a` 再乘
    /// `alpha`。画笔、合成器共用 [`blend_over`] 这一个定义。越界是 no-op。
    pub fn blend_pixel(&mut self, x: u32, y: u32, source: Color, alpha: f32) {
        let Some(index) = self.index(x, y) else {
            return;
        };
        blend_over(&mut self.data[index..index + 4], &source.to_rgba8(), alpha);
    }

    /// 按 `alpha` 把像素的 alpha 往 0 拉（橡皮）。RGB 不变。越界是 no-op。
    pub fn erase_pixel(&mut self, x: u32, y: u32, alpha: f32) {
        if !self.contains(x, y) {
            return;
        }
        let t = alpha.clamp(0.0, 1.0);
        if t <= 0.0 {
            return;
        }
        let dst = self.get_pixel(x, y);
        let a = (dst.a as f32 * (1.0 - t)).round() as u8;
        self.set_pixel(x, y, dst.with_alpha(a));
    }

    /// 把所有像素刷成 `color`。
    pub fn clear(&mut self, color: Color) {
        let rgba = color.to_rgba8();
        for pixel in self.data.chunks_exact_mut(4) {
            pixel.copy_from_slice(&rgba);
        }
    }

    /// 返回一块新尺寸的缓冲区，左上角对齐地拷贝重叠区域（不缩放、不插值）。
    ///
    /// 这是"改画布尺寸"的语义：超出新边界的像素被裁掉，新区域保持透明。
    ///
    /// TODO(v1.1): 需要缩放语义（缩略图 / 变换）时，在这里加一个显式的
    /// `resample`，不要让 `resize` 同时承担两种含义。
    pub fn resize(&self, width: u32, height: u32) -> Self {
        let mut out = Self::new(width, height);
        let copy_w = self.width.min(width);
        let copy_h = self.height.min(height);
        for y in 0..copy_h {
            for x in 0..copy_w {
                out.set_pixel(x, y, self.get_pixel(x, y));
            }
        }
        out
    }

    /// 把 `self` 以 `(dx, dy)` 为偏移画到一块 `width × height` 的透明缓冲区上，
    /// 返回新缓冲区；超出目标边界的像素裁掉。
    ///
    /// 这是"把图层的 `position` 烘进像素"的底层操作（画笔落笔前调用）：图层
    /// 小于文档时补大到文档尺寸，大于文档时保留原尺寸，两种情况下都保持内容
    /// 在屏幕上不动，并让图层重新与文档原点对齐（`position = 0`）。
    pub fn placed(&self, width: u32, height: u32, dx: i32, dy: i32) -> Self {
        let mut out = Self::new(width, height);
        for y in 0..self.height {
            for x in 0..self.width {
                let nx = x as i64 + dx as i64;
                let ny = y as i64 + dy as i64;
                if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                    continue;
                }
                out.set_pixel(nx as u32, ny as u32, self.get_pixel(x, y));
            }
        }
        out
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if !self.contains(x, y) {
            return None;
        }
        Some((y as usize * self.width as usize + x as usize) * 4)
    }
}

/// 直通 alpha 的 `source-over`，在 4 字节 RGBA 上**就地**写入 `dst`。
///
/// 画笔、橡皮、合成器共用这一个定义；`dst` / `src` 都必须是 `[r, g, b, a]`。
/// `src` 里 alpha 为 0 时是 no-op。
pub(crate) fn blend_over(dst: &mut [u8], src: &[u8], alpha: f32) {
    let sa = (src[3] as f32 / 255.0) * alpha.clamp(0.0, 1.0);
    if sa <= 0.0 {
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        dst[..4].copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for channel in 0..3 {
        let s = src[channel] as f32 / 255.0;
        let d = dst[channel] as f32 / 255.0;
        let out = (s * sa + d * da * (1.0 - sa)) / out_a;
        dst[channel] = (out.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    dst[3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
}

/// `width * height * 4`，溢出返回 `None`。
fn byte_len(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)
}

impl fmt::Debug for PixelBuffer {
    /// 只打印尺寸和字节数 —— 像素内容太长，`{:?}` 一个缓冲区不该刷屏。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PixelBuffer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.data.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_buffer_is_transparent_and_correctly_sized() {
        let buffer = PixelBuffer::new(3, 2);
        assert_eq!((buffer.width, buffer.height), (3, 2));
        assert_eq!(buffer.len(), 3 * 2 * 4);
        for y in 0..2 {
            for x in 0..3 {
                assert_eq!(buffer.get_pixel(x, y), Color::TRANSPARENT);
            }
        }
    }

    #[test]
    fn set_and_get_round_trip() {
        let mut buffer = PixelBuffer::new(2, 2);
        buffer.set_pixel(1, 0, Color::new(1, 2, 3, 4));
        assert_eq!(buffer.get_pixel(1, 0), Color::new(1, 2, 3, 4));
        // 相邻像素不受影响。
        assert_eq!(buffer.get_pixel(0, 0), Color::TRANSPARENT);
    }

    #[test]
    fn out_of_bounds_reads_transparent_and_writes_are_no_ops() {
        let mut buffer = PixelBuffer::filled(2, 2, Color::WHITE);
        assert_eq!(buffer.get_pixel(2, 0), Color::TRANSPARENT);
        assert_eq!(buffer.get_pixel(0, 2), Color::TRANSPARENT);
        buffer.set_pixel(9, 9, Color::BLACK);
        assert_eq!(buffer.get_pixel(0, 0), Color::WHITE, "边界内未被改动");
        assert!(!buffer.contains(2, 2));
        assert!(buffer.contains(1, 1));
    }

    #[test]
    fn clear_paints_every_pixel() {
        let mut buffer = PixelBuffer::new(2, 2);
        buffer.clear(Color::RED);
        for y in 0..2 {
            for x in 0..2 {
                assert_eq!(buffer.get_pixel(x, y), Color::RED);
            }
        }
    }

    #[test]
    fn resize_copies_the_overlap_and_leaves_new_space_transparent() {
        let mut buffer = PixelBuffer::filled(2, 2, Color::RED);
        buffer.set_pixel(1, 1, Color::WHITE);

        // 变大：旧内容在左上角，新区域透明。
        let bigger = buffer.resize(3, 3);
        assert_eq!((bigger.width, bigger.height), (3, 3));
        assert_eq!(bigger.get_pixel(0, 0), Color::RED);
        assert_eq!(bigger.get_pixel(1, 1), Color::WHITE);
        assert_eq!(bigger.get_pixel(2, 2), Color::TRANSPARENT);

        // 变小：超出新边界的像素被裁掉。
        let smaller = buffer.resize(1, 1);
        assert_eq!((smaller.width, smaller.height), (1, 1));
        assert_eq!(smaller.get_pixel(0, 0), Color::RED);
    }

    #[test]
    fn placed_blits_onto_a_larger_buffer_and_clips_the_edges() {
        let mut buffer = PixelBuffer::new(3, 1);
        buffer.set_pixel(0, 0, Color::RED);

        // 向右下放到 4×2：空出的一格透明，内容按偏移落下。
        let shifted = buffer.placed(4, 2, 1, 1);
        assert_eq!((shifted.width, shifted.height), (4, 2));
        assert_eq!(shifted.get_pixel(0, 0), Color::TRANSPARENT);
        assert_eq!(shifted.get_pixel(1, 1), Color::RED);

        // 向左移：内容被裁到边界。
        let clipped = buffer.placed(3, 1, -1, 0);
        assert_eq!(clipped.get_pixel(0, 0), Color::TRANSPARENT);
    }

    #[test]
    fn from_rgba8_rejects_a_mismatched_length() {
        assert!(PixelBuffer::from_rgba8(1, 1, vec![0; 4]).is_some());
        assert!(PixelBuffer::from_rgba8(1, 1, vec![0; 3]).is_none());
        assert!(PixelBuffer::from_rgba8(2, 2, vec![0; 4]).is_none());
    }

    #[test]
    fn blend_pixel_is_source_over() {
        let mut buffer = PixelBuffer::filled(1, 1, Color::WHITE);
        buffer.blend_pixel(0, 0, Color::RED, 0.5);
        let color = buffer.get_pixel(0, 0);
        assert_eq!(color.a, 255);
        assert_eq!(color.r, 255);
        assert!((color.g as i32 - 128).abs() <= 1, "g = {}", color.g);
        assert!((color.b as i32 - 128).abs() <= 1, "b = {}", color.b);
    }

    #[test]
    fn blend_pixel_ignores_a_fully_transparent_source() {
        let mut buffer = PixelBuffer::filled(1, 1, Color::WHITE);
        buffer.blend_pixel(0, 0, Color::TRANSPARENT, 1.0);
        assert_eq!(buffer.get_pixel(0, 0), Color::WHITE);
    }

    #[test]
    fn blend_pixel_on_an_empty_buffer_keeps_the_source_alpha() {
        let mut buffer = PixelBuffer::new(1, 1);
        buffer.blend_pixel(0, 0, Color::rgb(10, 20, 30), 0.5);
        let color = buffer.get_pixel(0, 0);
        assert_eq!(color.to_rgba8(), [10, 20, 30, 128]);
    }

    #[test]
    fn erase_pixel_lowers_alpha_and_keeps_rgb() {
        let mut buffer = PixelBuffer::filled(1, 1, Color::RED);
        buffer.erase_pixel(0, 0, 0.5);
        let color = buffer.get_pixel(0, 0);
        assert_eq!((color.r, color.g, color.b), (255, 0, 0));
        assert!((color.a as i32 - 128).abs() <= 1, "a = {}", color.a);
        // 全量擦除 -> 透明。
        buffer.erase_pixel(0, 0, 1.0);
        assert_eq!(buffer.get_pixel(0, 0).a, 0);
    }

    #[test]
    fn blend_and_erase_are_bounds_checked() {
        let mut buffer = PixelBuffer::new(1, 1);
        buffer.blend_pixel(9, 9, Color::RED, 1.0);
        buffer.erase_pixel(9, 9, 1.0);
        // 没 panic，也没动到边界内。
        assert_eq!(buffer.get_pixel(0, 0), Color::TRANSPARENT);
    }
}
