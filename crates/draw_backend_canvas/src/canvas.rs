use std::collections::HashMap;
use std::fmt;

use web_sys::{CanvasRenderingContext2d, HtmlImageElement};

use draw_core::{Color, Transform2D, ViewportSize};
use draw_render::{CornerRadii, DrawCommand, DrawList, Paint, RenderBackend, TextAlign, TextureId};

/// The Canvas font spec used for `DrawText` at `font_size` logical pixels.
///
/// A host-side `TextMeasurer` must measure with this exact spec so layout
/// baselines match what the backend draws.
pub fn font_spec(font_size: f32) -> String {
    format!("{font_size}px sans-serif")
}

/// Errors from the Canvas 2D backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasError {
    /// The context was not created from a canvas element.
    MissingCanvas,
}

impl fmt::Display for CanvasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCanvas => f.write_str("2d context is not attached to a canvas"),
        }
    }
}

impl std::error::Error for CanvasError {}

/// A [`RenderBackend`] that maps a [`DrawList`] onto the HTML Canvas 2D API.
///
/// # Coordinate handling
///
/// Geometry is authored in logical pixels. The backend scales the backing store
/// to `logical * scale_factor` and multiplies every transform by `scale_factor`,
/// so DPR never leaks into the core or the recorded IR.
pub struct Canvas2dBackend {
    ctx: CanvasRenderingContext2d,
    scale_factor: f32,
    transform: Transform2D,
    images: HashMap<TextureId, HtmlImageElement>,
}

impl Canvas2dBackend {
    pub fn new(ctx: CanvasRenderingContext2d) -> Self {
        Self {
            ctx,
            scale_factor: 1.0,
            transform: Transform2D::IDENTITY,
            images: HashMap::new(),
        }
    }

    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor.max(0.0);
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn context(&self) -> &CanvasRenderingContext2d {
        &self.ctx
    }

    /// Registers a decoded image so `DrawImage` commands can reference it.
    pub fn register_image(&mut self, texture: TextureId, image: HtmlImageElement) {
        self.images.insert(texture, image);
    }

    fn apply_transform(&self, transform: Transform2D) {
        let s = self.scale_factor as f64;
        let _ = self.ctx.set_transform(
            transform.x_axis.x as f64 * s,
            transform.x_axis.y as f64 * s,
            transform.y_axis.x as f64 * s,
            transform.y_axis.y as f64 * s,
            transform.origin.x as f64 * s,
            transform.origin.y as f64 * s,
        );
    }

    fn set_fill(&self, paint: &Paint) {
        self.ctx.set_fill_style_str(&css_color(paint.color));
    }

    fn set_stroke(&self, paint: &Paint, width: f32) {
        self.ctx.set_stroke_style_str(&css_color(paint.color));
        self.ctx.set_line_width(width as f64);
    }

    fn execute(&mut self, command: &DrawCommand) {
        match command {
            DrawCommand::Save => self.ctx.save(),
            DrawCommand::Restore => self.ctx.restore(),
            DrawCommand::SetTransform(transform) => {
                self.transform = *transform;
                self.apply_transform(*transform);
            }
            DrawCommand::SetOpacity(opacity) => self.ctx.set_global_alpha(*opacity as f64),
            DrawCommand::ClipRect(rect) => {
                // Clip rectangles are in viewport/logical space: build the path
                // in device space, then restore the logical transform. Canvas
                // keeps the clip region in device space afterwards.
                let _ = self.ctx.set_transform(
                    self.scale_factor as f64,
                    0.0,
                    0.0,
                    self.scale_factor as f64,
                    0.0,
                    0.0,
                );
                self.ctx.begin_path();
                self.ctx.rect(
                    rect.left() as f64,
                    rect.top() as f64,
                    rect.size.width as f64,
                    rect.size.height as f64,
                );
                self.ctx.clip();
                self.apply_transform(self.transform);
            }
            DrawCommand::FillRect { rect, paint } => {
                self.set_fill(paint);
                self.ctx.fill_rect(
                    rect.left() as f64,
                    rect.top() as f64,
                    rect.size.width as f64,
                    rect.size.height as f64,
                );
            }
            DrawCommand::StrokeRect { rect, paint, width } => {
                self.set_stroke(paint, *width);
                self.ctx.stroke_rect(
                    rect.left() as f64,
                    rect.top() as f64,
                    rect.size.width as f64,
                    rect.size.height as f64,
                );
            }
            DrawCommand::Line {
                from,
                to,
                paint,
                width,
            } => {
                self.set_stroke(paint, *width);
                self.ctx.begin_path();
                self.ctx.move_to(from.x as f64, from.y as f64);
                self.ctx.line_to(to.x as f64, to.y as f64);
                self.ctx.stroke();
            }
            DrawCommand::FillCircle {
                center,
                radius,
                paint,
            } => {
                self.set_fill(paint);
                self.path_circle(*center, *radius);
                self.ctx.fill();
            }
            DrawCommand::StrokeCircle {
                center,
                radius,
                paint,
                width,
            } => {
                self.set_stroke(paint, *width);
                self.path_circle(*center, *radius);
                self.ctx.stroke();
            }
            DrawCommand::FillRoundedRect {
                rect,
                corners,
                paint,
            } => {
                self.set_fill(paint);
                self.path_rounded_rect(*rect, *corners);
                self.ctx.fill();
            }
            DrawCommand::StrokeRoundedRect {
                rect,
                corners,
                paint,
                width,
            } => {
                self.set_stroke(paint, *width);
                self.path_rounded_rect(*rect, *corners);
                self.ctx.stroke();
            }
            DrawCommand::DrawImage {
                texture,
                destination,
                source,
                paint,
            } => {
                let Some(image) = self.images.get(texture).cloned() else {
                    return;
                };
                let (sx, sy, sw, sh) = match source {
                    Some(source) => (
                        source.left(),
                        source.top(),
                        source.size.width,
                        source.size.height,
                    ),
                    None => (0.0, 0.0, image.width() as f32, image.height() as f32),
                };
                self.ctx.set_global_alpha(paint.color.a as f64);
                let _ = self
                    .ctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        &image,
                        sx as f64,
                        sy as f64,
                        sw as f64,
                        sh as f64,
                        destination.left() as f64,
                        destination.top() as f64,
                        destination.size.width as f64,
                        destination.size.height as f64,
                    );
            }
            DrawCommand::DrawText {
                text,
                position,
                font_size,
                align,
                paint,
            } => {
                self.set_fill(paint);
                self.ctx.set_font(&font_spec(*font_size));
                self.ctx.set_text_align(match align {
                    TextAlign::Left => "left",
                    TextAlign::Center => "center",
                    TextAlign::Right => "right",
                });
                let _ = self
                    .ctx
                    .fill_text(text, position.x as f64, position.y as f64);
            }
        }
    }

    fn path_circle(&self, center: draw_core::Vec2, radius: f32) {
        self.ctx.begin_path();
        let _ = self.ctx.arc(
            center.x as f64,
            center.y as f64,
            radius as f64,
            0.0,
            std::f64::consts::TAU,
        );
    }

    /// Builds a rounded-rectangle path with per-corner quarter-circle corners.
    fn path_rounded_rect(&self, rect: draw_core::Rect, corners: CornerRadii) {
        let half_pi = std::f64::consts::FRAC_PI_2;
        let (left, top) = (rect.left() as f64, rect.top() as f64);
        let (right, bottom) = (rect.right() as f64, rect.bottom() as f64);
        let max = (rect.size.width * 0.5).min(rect.size.height * 0.5);
        let radii = corners.clamp(max);
        let (tl, tr, br, bl) = (
            radii.top_left as f64,
            radii.top_right as f64,
            radii.bottom_right as f64,
            radii.bottom_left as f64,
        );

        self.ctx.begin_path();
        self.ctx.move_to(left + tl, top);
        self.ctx.line_to(right - tr, top);
        if tr > 0.0 {
            let _ = self.ctx.arc(right - tr, top + tr, tr, -half_pi, 0.0);
        }
        self.ctx.line_to(right, bottom - br);
        if br > 0.0 {
            let _ = self.ctx.arc(right - br, bottom - br, br, 0.0, half_pi);
        }
        self.ctx.line_to(left + bl, bottom);
        if bl > 0.0 {
            let _ = self
                .ctx
                .arc(left + bl, bottom - bl, bl, half_pi, 2.0 * half_pi);
        }
        self.ctx.line_to(left, top + tl);
        if tl > 0.0 {
            let _ = self
                .ctx
                .arc(left + tl, top + tl, tl, 2.0 * half_pi, 3.0 * half_pi);
        }
        self.ctx.close_path();
    }
}

impl RenderBackend for Canvas2dBackend {
    type Error = CanvasError;

    fn begin_frame(&mut self, viewport: ViewportSize) -> Result<(), Self::Error> {
        let canvas = self.ctx.canvas().ok_or(CanvasError::MissingCanvas)?;
        let device = viewport.device_size(self.scale_factor);
        canvas.set_width(device.width.round().max(0.0) as u32);
        canvas.set_height(device.height.round().max(0.0) as u32);

        self.transform = Transform2D::IDENTITY;
        self.ctx.set_global_alpha(1.0);
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

fn css_color(color: Color) -> String {
    let [r, g, b, a] = color.to_rgba8();
    if a == 255 {
        format!("rgb({r},{g},{b})")
    } else {
        format!("rgba({r},{g},{b},{:.4})", a as f32 / 255.0)
    }
}
