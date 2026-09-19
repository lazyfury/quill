use std::collections::HashMap;
use std::fmt;

use objc2_core_foundation::{
    CFAttributedString, CFDictionary, CFRetained, CFString, CFType, CGAffineTransform, CGFloat,
    CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGAffineTransformInvert, CGBitmapContextCreate, CGBitmapContextCreateImage, CGColor,
    CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
};
use objc2_core_text::{kCTFontAttributeName, kCTForegroundColorAttributeName, CTFont, CTLine};

use draw_core::{Rect, Transform2D, Vec2, Viewport};
use draw_render::{DrawCommand, DrawList, Paint, RenderBackend, TextAlign, TextureId};

/// Errors from the Core Graphics backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreGraphicsError {
    /// The bitmap context could not be created.
    ContextCreationFailed,
}

impl fmt::Display for CoreGraphicsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextCreationFailed => f.write_str("failed to create CG bitmap context"),
        }
    }
}

impl std::error::Error for CoreGraphicsError {}

const BITMAP_INFO: u32 =
    CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0;

/// A [`RenderBackend`] that draws a [`DrawList`] into a Core Graphics bitmap
/// context.
///
/// Pixels are premultiplied BGRA (little-endian), so the buffer can be read
/// directly for tests, encoded to PNG, or turned into a `CGImage` for display
/// via `NSImage`.
pub struct CoreGraphicsBackend {
    context: Option<CFRetained<CGContext>>,
    buffer: Vec<u8>,
    width: usize,
    height: usize,
    scale_factor: f32,
    /// Base transform from logical (top-left, y-down) to device space.
    base: CGAffineTransform,
    images: HashMap<TextureId, CFRetained<CGImage>>,
}

impl Default for CoreGraphicsBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreGraphicsBackend {
    pub fn new() -> Self {
        Self {
            context: None,
            buffer: Vec::new(),
            width: 0,
            height: 0,
            scale_factor: 1.0,
            base: identity(),
            images: HashMap::new(),
        }
    }

    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor.max(0.0);
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Backing buffer size in device pixels.
    pub fn pixel_size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Raw premultiplied BGRA pixels (little-endian).
    pub fn pixels(&self) -> &[u8] {
        &self.buffer
    }

    pub fn register_image(&mut self, texture: TextureId, image: CFRetained<CGImage>) {
        self.images.insert(texture, image);
    }

    /// Creates a `CGImage` view of the current buffer.
    pub fn image(&self) -> Option<CFRetained<CGImage>> {
        CGBitmapContextCreateImage(self.context.as_deref())
    }

    fn ensure_context(&mut self) {
        if self.context.is_some() {
            return;
        }
        let Some(color_space) = CGColorSpace::new_device_rgb() else {
            return;
        };
        let context = unsafe {
            CGBitmapContextCreate(
                self.buffer.as_mut_ptr().cast(),
                self.width,
                self.height,
                8,
                self.width * 4,
                Some(&color_space),
                BITMAP_INFO,
            )
        };
        self.context = context;
    }

    fn apply_transform(&self, transform: Transform2D) {
        let Some(context) = self.context.as_deref() else {
            return;
        };
        // Reset the CTM to identity, then re-apply the base and the logical
        // transform so each `SetTransform` is absolute.
        let current = CGContext::ctm(Some(context));
        CGContext::concat_ctm(Some(context), CGAffineTransformInvert(current));
        CGContext::concat_ctm(Some(context), self.base);
        CGContext::concat_ctm(Some(context), affine(transform));
    }

    fn set_fill(&self, paint: &Paint) {
        if let Some(context) = self.context.as_deref() {
            let c = paint.color;
            CGContext::set_rgb_fill_color(
                Some(context),
                c.r as CGFloat,
                c.g as CGFloat,
                c.b as CGFloat,
                c.a as CGFloat,
            );
        }
    }

    fn set_stroke(&self, paint: &Paint, width: f32) {
        if let Some(context) = self.context.as_deref() {
            let c = paint.color;
            CGContext::set_rgb_stroke_color(
                Some(context),
                c.r as CGFloat,
                c.g as CGFloat,
                c.b as CGFloat,
                c.a as CGFloat,
            );
            CGContext::set_line_width(Some(context), width as CGFloat);
        }
    }

    fn ellipse_path(&self, center: Vec2, radius: f32) {
        let Some(context) = self.context.as_deref() else {
            return;
        };
        let rect = CGRect {
            origin: CGPoint {
                x: (center.x - radius) as CGFloat,
                y: (center.y - radius) as CGFloat,
            },
            size: CGSize {
                width: (radius * 2.0) as CGFloat,
                height: (radius * 2.0) as CGFloat,
            },
        };
        CGContext::begin_path(Some(context));
        CGContext::add_ellipse_in_rect(Some(context), rect);
    }

    fn draw_text(
        &self,
        text: &str,
        position: Vec2,
        font_size: f32,
        align: TextAlign,
        paint: &Paint,
    ) {
        let Some(context) = self.context.as_deref() else {
            return;
        };
        if text.is_empty() {
            return;
        }

        let string = CFString::from_str(text);
        let font_name = CFString::from_str("Helvetica");
        let font = unsafe { CTFont::with_name(&font_name, font_size as CGFloat, std::ptr::null()) };
        let color = CGColor::new_srgb(
            paint.color.r as CGFloat,
            paint.color.g as CGFloat,
            paint.color.b as CGFloat,
            paint.color.a as CGFloat,
        );

        // Treat the CF objects as the common `CFType` root for the dictionary.
        let font_type: CFRetained<CFType> = unsafe { CFRetained::cast_unchecked(font) };
        let color_type: CFRetained<CFType> = unsafe { CFRetained::cast_unchecked(color) };
        let keys: [&CFString; 2] =
            unsafe { [kCTFontAttributeName, kCTForegroundColorAttributeName] };
        let values: [&CFType; 2] = [&font_type, &color_type];
        let attributes = CFDictionary::from_slices(&keys, &values);
        let attributes: CFRetained<CFDictionary> =
            unsafe { CFRetained::cast_unchecked(attributes) };

        let Some(attributed) =
            (unsafe { CFAttributedString::new(None, Some(&string), Some(&attributes)) })
        else {
            return;
        };
        let line = unsafe { CTLine::with_attributed_string(&attributed) };
        let width = unsafe {
            line.typographic_bounds(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        let dx = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => -(width as f32) * 0.5,
            TextAlign::Right => -(width as f32),
        };

        // The context is y-down, so flip the text matrix to keep glyphs upright.
        CGContext::set_text_matrix(
            Some(context),
            CGAffineTransform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: -1.0,
                tx: 0.0,
                ty: 0.0,
            },
        );
        CGContext::set_text_position(
            Some(context),
            (position.x + dx) as CGFloat,
            position.y as CGFloat,
        );
        unsafe { line.draw(context) };
    }

    fn execute(&self, command: &DrawCommand) {
        let Some(context) = self.context.as_deref() else {
            return;
        };
        match command {
            DrawCommand::Save => CGContext::save_g_state(Some(context)),
            DrawCommand::Restore => CGContext::restore_g_state(Some(context)),
            DrawCommand::SetTransform(transform) => self.apply_transform(*transform),
            DrawCommand::SetOpacity(opacity) => {
                CGContext::set_alpha(Some(context), *opacity as CGFloat)
            }
            DrawCommand::ClipRect(rect) => {
                let current = CGContext::ctm(Some(context));
                CGContext::concat_ctm(Some(context), CGAffineTransformInvert(current));
                CGContext::concat_ctm(Some(context), self.base);
                CGContext::clip_to_rect(Some(context), cg_rect(*rect));
            }
            DrawCommand::FillRect { rect, paint } => {
                self.set_fill(paint);
                CGContext::fill_rect(Some(context), cg_rect(*rect));
            }
            DrawCommand::StrokeRect { rect, paint, width } => {
                self.set_stroke(paint, *width);
                CGContext::stroke_rect(Some(context), cg_rect(*rect));
            }
            DrawCommand::FillCircle {
                center,
                radius,
                paint,
            } => {
                self.set_fill(paint);
                self.ellipse_path(*center, *radius);
                CGContext::fill_path(Some(context));
            }
            DrawCommand::StrokeCircle {
                center,
                radius,
                paint,
                width,
            } => {
                self.set_stroke(paint, *width);
                self.ellipse_path(*center, *radius);
                CGContext::stroke_path(Some(context));
            }
            DrawCommand::DrawImage {
                texture,
                destination,
                paint,
                ..
            } => {
                if let Some(image) = self.images.get(texture) {
                    CGContext::set_alpha(Some(context), paint.color.a as CGFloat);
                    CGContext::draw_image(Some(context), cg_rect(*destination), Some(image));
                }
            }
            DrawCommand::DrawText {
                text,
                position,
                font_size,
                align,
                paint,
            } => self.draw_text(text, *position, *font_size, *align, paint),
        }
    }
}

impl RenderBackend for CoreGraphicsBackend {
    type Error = CoreGraphicsError;

    fn begin_frame(&mut self, viewport: Viewport) -> Result<(), Self::Error> {
        let device = viewport.device_size(self.scale_factor);
        let width = device.width.round().max(1.0) as usize;
        let height = device.height.round().max(1.0) as usize;

        if width != self.width || height != self.height || self.context.is_none() {
            self.width = width;
            self.height = height;
            self.buffer = vec![0u8; width * height * 4];
            self.context = None;
            self.ensure_context();
        }
        if self.context.is_none() {
            return Err(CoreGraphicsError::ContextCreationFailed);
        }

        for byte in self.buffer.iter_mut() {
            *byte = 0;
        }

        // base = translate(0, height) * scale(dpr, -dpr): y-down logical space.
        self.base = CGAffineTransform {
            a: self.scale_factor as CGFloat,
            b: 0.0,
            c: 0.0,
            d: -(self.scale_factor as CGFloat),
            tx: 0.0,
            ty: height as CGFloat,
        };
        self.apply_transform(Transform2D::IDENTITY);
        Ok(())
    }

    fn submit(&mut self, list: &DrawList) -> Result<(), Self::Error> {
        for command in list.commands() {
            self.execute(command);
        }
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn identity() -> CGAffineTransform {
    CGAffineTransform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    }
}

fn affine(transform: Transform2D) -> CGAffineTransform {
    CGAffineTransform {
        a: transform.x_axis.x as CGFloat,
        b: transform.x_axis.y as CGFloat,
        c: transform.y_axis.x as CGFloat,
        d: transform.y_axis.y as CGFloat,
        tx: transform.origin.x as CGFloat,
        ty: transform.origin.y as CGFloat,
    }
}

fn cg_rect(rect: Rect) -> CGRect {
    CGRect {
        origin: CGPoint {
            x: rect.left() as CGFloat,
            y: rect.top() as CGFloat,
        },
        size: CGSize {
            width: rect.size.width as CGFloat,
            height: rect.size.height as CGFloat,
        },
    }
}
