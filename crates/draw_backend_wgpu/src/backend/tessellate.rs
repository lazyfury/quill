//! Turning `DrawCommand`s into vertices and draw ranges.
//!
//! Everything here runs on the CPU during `submit`: the draw state
//! (transform/opacity/clip) is tracked, primitives are tessellated into
//! triangles, and positions are converted to NDC. The GPU only rasterizes the
//! resulting textured quads.

use super::*;
use draw_core::{FontWeight, Size};
use draw_render::{CornerRadii, DrawCommand, TextAlign};

impl WgpuBackend {
    pub(super) fn bind_group_for(&self, surface: Surface) -> &wgpu::BindGroup {
        match surface {
            Surface::Solid => &self.white_bind_group,
            Surface::Font => &self.font_bind_group,
            Surface::Texture(id) => self.textures.get(&id).unwrap_or(&self.white_bind_group),
        }
    }

    pub(super) fn execute(&mut self, command: &DrawCommand) {
        match command {
            DrawCommand::Save => self.stack.push(self.state),
            DrawCommand::Restore => {
                if let Some(state) = self.stack.pop() {
                    self.state = state;
                }
            }
            DrawCommand::SetTransform(transform) => self.state.transform = *transform,
            DrawCommand::SetOpacity(opacity) => self.state.opacity = opacity.clamp(0.0, 1.0),
            DrawCommand::ClipRect(rect) => self.state.clip = Some(*rect),
            DrawCommand::FillRect { rect, paint } => {
                let color = self.solid_color(paint);
                self.quad(*rect, SOLID_UV, color, Surface::Solid);
            }
            DrawCommand::StrokeRect { rect, paint, width } => {
                let color = self.solid_color(paint);
                self.stroke_rect(*rect, *width, color);
            }
            DrawCommand::Line {
                from,
                to,
                paint,
                width,
            } => {
                let color = self.solid_color(paint);
                self.line(*from, *to, *width, color);
            }
            DrawCommand::FillCircle {
                center,
                radius,
                paint,
            } => {
                let color = self.solid_color(paint);
                self.fill_circle(*center, *radius, color);
            }
            DrawCommand::StrokeCircle {
                center,
                radius,
                paint,
                width,
            } => {
                let color = self.solid_color(paint);
                self.stroke_circle(*center, *radius, *width, color);
            }
            DrawCommand::FillRoundedRect {
                rect,
                corners,
                paint,
            } => {
                let color = self.solid_color(paint);
                self.fill_rounded_rect(*rect, *corners, color);
            }
            DrawCommand::StrokeRoundedRect {
                rect,
                corners,
                paint,
                width,
            } => {
                let color = self.solid_color(paint);
                self.stroke_rounded_rect(*rect, *corners, *width, color);
            }
            DrawCommand::DrawImage {
                texture,
                destination,
                source,
                paint,
            } => {
                if let Some(uv) = self.image_uv(*texture, *source) {
                    let color = self.solid_color(paint);
                    self.quad(*destination, uv, color, Surface::Texture(*texture));
                }
            }
            DrawCommand::DrawText {
                text,
                position,
                font_size,
                weight,
                align,
                paint,
            } => {
                let color = self.solid_color(paint);
                self.draw_text(text, *position, *font_size, *weight, *align, color);
            }
        }
    }

    pub(super) fn solid_color(&self, paint: &Paint) -> [f32; 4] {
        let color = paint.color;
        [
            color.r,
            color.g,
            color.b,
            (color.a * self.state.opacity).clamp(0.0, 1.0),
        ]
    }

    pub(super) fn clip_result(&self) -> ClipResult {
        let Some(rect) = self.state.clip else {
            return ClipResult::Full;
        };
        let scale = self.scale_factor;
        let x0 = (rect.left() * scale).floor().clamp(0.0, self.device_width);
        let y0 = (rect.top() * scale).floor().clamp(0.0, self.device_height);
        let x1 = (rect.right() * scale).ceil().clamp(0.0, self.device_width);
        let y1 = (rect.bottom() * scale)
            .ceil()
            .clamp(0.0, self.device_height);
        if x1 <= x0 || y1 <= y0 {
            return ClipResult::Empty;
        }
        ClipResult::Scissor([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
    }

    /// Captures the vertex start and scissor for a primitive, or `None` when
    /// the current clip is empty.
    pub(super) fn begin(&self) -> Option<(u32, Option<[u32; 4]>)> {
        match self.clip_result() {
            ClipResult::Empty => None,
            ClipResult::Full => Some((self.vertices.len() as u32, None)),
            ClipResult::Scissor(rect) => Some((self.vertices.len() as u32, Some(rect))),
        }
    }

    pub(super) fn finish(&mut self, geometry: (u32, Option<[u32; 4]>), surface: Surface) {
        let (start, scissor) = geometry;
        let end = self.vertices.len() as u32;
        if end > start {
            self.ranges.push(DrawRange {
                vertices: start..end,
                surface,
                scissor,
            });
        }
    }

    pub(super) fn push_vertex(&mut self, local: Vec2, uv: [f32; 2], color: [f32; 4]) {
        let logical = self.state.transform.transform_point(local);
        let device = Vec2::new(logical.x * self.scale_factor, logical.y * self.scale_factor);
        let position = [
            device.x / self.device_width * 2.0 - 1.0,
            1.0 - device.y / self.device_height * 2.0,
        ];
        self.vertices.push(Vertex {
            position,
            uv,
            color,
        });
    }

    pub(super) fn quad(&mut self, rect: Rect, uv: [f32; 4], color: [f32; 4], surface: Surface) {
        if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
            return;
        }
        let Some(geometry) = self.begin() else {
            return;
        };
        let min = rect.min();
        let max = rect.max();
        let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
        let uvs = [
            [uv[0], uv[1]],
            [uv[2], uv[1]],
            [uv[2], uv[3]],
            [uv[0], uv[3]],
        ];
        for index in [0usize, 1, 2, 0, 2, 3] {
            self.push_vertex(corners[index], uvs[index], color);
        }
        self.finish(geometry, surface);
    }

    pub(super) fn stroke_rect(&mut self, rect: Rect, width: f32, color: [f32; 4]) {
        if width <= 0.0 || rect.is_empty() {
            return;
        }
        let (left, top, right, bottom) = (rect.left(), rect.top(), rect.right(), rect.bottom());
        let inner_top = (top + width).min(bottom);
        let inner_bottom = (bottom - width).max(top);
        let inner_left = (left + width).min(right);
        let inner_right = (right - width).max(left);

        self.quad(
            Rect::from_min_max(Vec2::new(left, top), Vec2::new(right, inner_top)),
            SOLID_UV,
            color,
            Surface::Solid,
        );
        self.quad(
            Rect::from_min_max(Vec2::new(left, inner_bottom), Vec2::new(right, bottom)),
            SOLID_UV,
            color,
            Surface::Solid,
        );
        self.quad(
            Rect::from_min_max(
                Vec2::new(left, inner_top),
                Vec2::new(inner_left, inner_bottom),
            ),
            SOLID_UV,
            color,
            Surface::Solid,
        );
        self.quad(
            Rect::from_min_max(
                Vec2::new(inner_right, inner_top),
                Vec2::new(right, inner_bottom),
            ),
            SOLID_UV,
            color,
            Surface::Solid,
        );
    }

    /// Tessellates a line segment as a thin quad with square caps. A degenerate
    /// segment (zero length) is drawn as a small dot so round joins/caps stay
    /// visible.
    pub(super) fn line(&mut self, from: Vec2, to: Vec2, width: f32, color: [f32; 4]) {
        if width <= 0.0 {
            return;
        }
        let direction = to - from;
        let length = direction.length();
        if length <= f32::EPSILON {
            self.fill_circle(from, width * 0.5, color);
            return;
        }
        let normal = Vec2::new(-direction.y, direction.x) / length * (width * 0.5);
        let Some(geometry) = self.begin() else {
            return;
        };
        let corners = [from + normal, from - normal, to - normal, to + normal];
        for index in [0usize, 1, 2, 0, 2, 3] {
            self.push_vertex(corners[index], [0.5, 0.5], color);
        }
        self.finish(geometry, Surface::Solid);
    }

    pub(super) fn fill_circle(&mut self, center: Vec2, radius: f32, color: [f32; 4]) {
        if radius <= 0.0 {
            return;
        }
        let Some(geometry) = self.begin() else {
            return;
        };
        let step = std::f32::consts::TAU / CIRCLE_SEGMENTS as f32;
        for segment in 0..CIRCLE_SEGMENTS {
            let a0 = step * segment as f32;
            let a1 = step * (segment + 1) as f32;
            self.push_vertex(center, [0.5, 0.5], color);
            self.push_vertex(circle_point(center, radius, a0), [0.5, 0.5], color);
            self.push_vertex(circle_point(center, radius, a1), [0.5, 0.5], color);
        }
        self.finish(geometry, Surface::Solid);
    }

    pub(super) fn stroke_circle(&mut self, center: Vec2, radius: f32, width: f32, color: [f32; 4]) {
        if radius <= 0.0 || width <= 0.0 {
            return;
        }
        let Some(geometry) = self.begin() else {
            return;
        };
        let inner = (radius - width * 0.5).max(0.0);
        let outer = radius + width * 0.5;
        let step = std::f32::consts::TAU / CIRCLE_SEGMENTS as f32;
        for segment in 0..CIRCLE_SEGMENTS {
            let a0 = step * segment as f32;
            let a1 = step * (segment + 1) as f32;
            let i0 = circle_point(center, inner, a0);
            let i1 = circle_point(center, inner, a1);
            let o0 = circle_point(center, outer, a0);
            let o1 = circle_point(center, outer, a1);
            self.push_vertex(i0, [0.5, 0.5], color);
            self.push_vertex(o0, [0.5, 0.5], color);
            self.push_vertex(o1, [0.5, 0.5], color);
            self.push_vertex(i0, [0.5, 0.5], color);
            self.push_vertex(o1, [0.5, 0.5], color);
            self.push_vertex(i1, [0.5, 0.5], color);
        }
        self.finish(geometry, Surface::Solid);
    }

    /// Tessellates a filled rounded rectangle as a triangle fan around its
    /// center (the shape is convex).
    pub(super) fn fill_rounded_rect(&mut self, rect: Rect, corners: CornerRadii, color: [f32; 4]) {
        if rect.is_empty() {
            return;
        }
        let Some(geometry) = self.begin() else {
            return;
        };
        let points = rounded_rect_points(rect, corners);
        let center = rect.center();
        for index in 0..points.len() {
            let next = (index + 1) % points.len();
            self.push_vertex(center, [0.5, 0.5], color);
            self.push_vertex(points[index], [0.5, 0.5], color);
            self.push_vertex(points[next], [0.5, 0.5], color);
        }
        self.finish(geometry, Surface::Solid);
    }

    /// Tessellates a stroked rounded rectangle as a ring between the outer and
    /// inset polygons.
    pub(super) fn stroke_rounded_rect(
        &mut self,
        rect: Rect,
        corners: CornerRadii,
        width: f32,
        color: [f32; 4],
    ) {
        if width <= 0.0 || rect.is_empty() {
            return;
        }
        let outer = rounded_rect_points(rect, corners);
        let center = rect.center();
        let inner_rect = Rect::from_min_max(
            Vec2::new(
                (rect.left() + width).min(center.x),
                (rect.top() + width).min(center.y),
            ),
            Vec2::new(
                (rect.right() - width).max(center.x),
                (rect.bottom() - width).max(center.y),
            ),
        );
        if inner_rect.is_empty() {
            self.fill_rounded_rect(rect, corners, color);
            return;
        }
        let inner = rounded_rect_points(inner_rect, corners.inset(width));
        if outer.len() != inner.len() {
            self.fill_rounded_rect(rect, corners, color);
            return;
        }
        let Some(geometry) = self.begin() else {
            return;
        };
        for index in 0..outer.len() {
            let next = (index + 1) % outer.len();
            let (o0, o1) = (outer[index], outer[next]);
            let (i0, i1) = (inner[index], inner[next]);
            self.push_vertex(o0, [0.5, 0.5], color);
            self.push_vertex(i0, [0.5, 0.5], color);
            self.push_vertex(i1, [0.5, 0.5], color);
            self.push_vertex(o0, [0.5, 0.5], color);
            self.push_vertex(i1, [0.5, 0.5], color);
            self.push_vertex(o1, [0.5, 0.5], color);
        }
        self.finish(geometry, Surface::Solid);
    }

    pub(super) fn image_uv(&self, texture: TextureId, source: Option<Rect>) -> Option<[f32; 4]> {
        let &(width, height) = self.texture_sizes.get(&texture)?;
        if width == 0 || height == 0 {
            return None;
        }
        let (width, height) = (width as f32, height as f32);
        let (sx, sy, sw, sh) = match source {
            Some(rect) => (rect.left(), rect.top(), rect.size.width, rect.size.height),
            None => (0.0, 0.0, width, height),
        };
        Some([
            sx / width,
            sy / height,
            (sx + sw) / width,
            (sy + sh) / height,
        ])
    }

    pub(super) fn draw_text(
        &mut self,
        text: &str,
        position: Vec2,
        font_size: f32,
        weight: FontWeight,
        align: TextAlign,
        color: [f32; 4],
    ) {
        if font_size <= 0.0 || text.is_empty() {
            return;
        }
        // Shaped advances drive both alignment and glyph placement (kerning,
        // ligatures and bidi are resolved by the font under the given size).
        let glyphs = self.font.shape(text, font_size, weight);
        let total: f32 = glyphs.iter().map(|slot| slot.advance).sum();
        let mut pen = match align {
            TextAlign::Left => position.x,
            TextAlign::Center => position.x - total * 0.5,
            TextAlign::Right => position.x - total,
        };

        // Glyphs are rasterized on the integer device-pixel grid, and the font
        // atlas is sampled with nearest filtering. Placing a glyph at a
        // fractional device offset would therefore resample it: diagonal edges
        // alias and edge texel columns are skipped or duplicated. Snap each
        // glyph's top-left to the device grid so the atlas maps 1:1.
        let scale = self.scale_factor.max(f32::MIN_POSITIVE);
        let snap = |value: f32| (value * scale).round() / scale;

        for slot in glyphs {
            if slot.size[0] > 0.0 && slot.size[1] > 0.0 {
                // `position` is the baseline; `offset` is relative to the pen
                // with y down, so it already accounts for the font's ascent.
                let min = Vec2::new(
                    snap(pen + slot.offset[0]),
                    snap(position.y + slot.offset[1]),
                );
                let rect = Rect::from_min_size(min, Size::new(slot.size[0], slot.size[1]));
                self.quad(rect, slot.uv, color, Surface::Font);
            }
            pen += slot.advance;
        }
    }
}

pub(super) fn circle_point(center: Vec2, radius: f32, angle: f32) -> Vec2 {
    Vec2::new(
        center.x + angle.cos() * radius,
        center.y + angle.sin() * radius,
    )
}

/// Segments per rounded corner (kept small; UI radii are a few pixels).
const CORNER_SEGMENTS: usize = 8;

/// Polygon outline of a rounded rectangle, clockwise in y-down space.
///
/// Consecutive points are joined by straight edges, so the four straight sides
/// fall out of connecting one corner's last point to the next corner's first.
/// Each corner always contributes the same number of points (even at radius
/// zero) so outer and inner rings line up for stroking.
fn rounded_rect_points(rect: Rect, corners: CornerRadii) -> Vec<Vec2> {
    let min = rect.min();
    let max = rect.max();
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return Vec::new();
    }
    let limit = (rect.size.width * 0.5).min(rect.size.height * 0.5);
    let radii = corners.clamp(limit);

    let half_pi = std::f32::consts::FRAC_PI_2;
    let pi = std::f32::consts::PI;
    let corners = [
        (
            Vec2::new(min.x + radii.top_left, min.y + radii.top_left),
            pi,
            pi + half_pi,
            radii.top_left,
        ),
        (
            Vec2::new(max.x - radii.top_right, min.y + radii.top_right),
            pi + half_pi,
            pi * 2.0,
            radii.top_right,
        ),
        (
            Vec2::new(max.x - radii.bottom_right, max.y - radii.bottom_right),
            0.0,
            half_pi,
            radii.bottom_right,
        ),
        (
            Vec2::new(min.x + radii.bottom_left, max.y - radii.bottom_left),
            half_pi,
            pi,
            radii.bottom_left,
        ),
    ];

    let mut points = Vec::with_capacity(CORNER_SEGMENTS * corners.len() + 1);
    for (center, start, end, radius) in corners {
        for step in 0..=CORNER_SEGMENTS {
            let t = start + (end - start) * (step as f32 / CORNER_SEGMENTS as f32);
            points.push(circle_point(center, radius, t));
        }
    }
    points
}
