use draw_core::{FontWeight, Rect, Transform2D, Vec2};

use crate::command::{CornerRadii, DrawCommand, Paint, TextAlign};
use crate::texture::TextureId;

/// An ordered, backend-neutral list of [`DrawCommand`]s.
///
/// The same scene painted twice produces the same `DrawList`; this is the basis
/// for golden tests and for backend replaceability.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawList {
    commands: Vec<DrawCommand>,
}

impl DrawList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            commands: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, command: DrawCommand) {
        self.commands.push(command);
    }

    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn iter(&self) -> std::slice::Iter<'_, DrawCommand> {
        self.commands.iter()
    }

    pub fn into_commands(self) -> Vec<DrawCommand> {
        self.commands
    }
}

impl From<Vec<DrawCommand>> for DrawList {
    fn from(commands: Vec<DrawCommand>) -> Self {
        Self { commands }
    }
}

impl IntoIterator for DrawList {
    type Item = DrawCommand;
    type IntoIter = std::vec::IntoIter<DrawCommand>;

    fn into_iter(self) -> Self::IntoIter {
        self.commands.into_iter()
    }
}

impl<'a> IntoIterator for &'a DrawList {
    type Item = &'a DrawCommand;
    type IntoIter = std::slice::Iter<'a, DrawCommand>;

    fn into_iter(self) -> Self::IntoIter {
        self.commands.iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PaintState {
    transform: Transform2D,
    opacity: f32,
    clip: Option<Rect>,
}

impl Default for PaintState {
    fn default() -> Self {
        Self {
            transform: Transform2D::IDENTITY,
            opacity: 1.0,
            clip: None,
        }
    }
}

/// The builder nodes use to emit a [`DrawList`].
///
/// It tracks the current transform, opacity and clip so callers can query the
/// active state, and emits matching [`DrawCommand`]s. `save`/`restore` are
/// balanced: `restore` on an empty stack is a no-op and emits nothing.
#[derive(Debug, Clone, Default)]
pub struct PaintContext {
    commands: Vec<DrawCommand>,
    stack: Vec<PaintState>,
    state: PaintState,
}

impl PaintContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            commands: Vec::with_capacity(capacity),
            ..Self::default()
        }
    }

    /// Returns the accumulated command list, consuming the context.
    pub fn into_draw_list(self) -> DrawList {
        DrawList::from(self.commands)
    }

    pub fn draw_list(&self) -> &[DrawCommand] {
        &self.commands
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    // -- state stack -------------------------------------------------------

    pub fn save(&mut self) {
        self.stack.push(self.state);
        self.commands.push(DrawCommand::Save);
    }

    pub fn restore(&mut self) {
        if let Some(state) = self.stack.pop() {
            self.state = state;
            self.commands.push(DrawCommand::Restore);
        }
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    // -- state -------------------------------------------------------------

    pub fn set_transform(&mut self, transform: Transform2D) {
        self.state.transform = transform;
        self.commands.push(DrawCommand::SetTransform(transform));
    }

    /// Multiplies the current transform by `transform` (applied after it) and
    /// emits the resulting absolute transform.
    pub fn transform(&mut self, transform: Transform2D) {
        self.set_transform(self.state.transform * transform);
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.state.opacity = opacity;
        self.commands.push(DrawCommand::SetOpacity(opacity));
    }

    /// Sets the clip rectangle in viewport/logical space.
    pub fn clip_rect(&mut self, rect: Rect) {
        self.state.clip = Some(rect);
        self.commands.push(DrawCommand::ClipRect(rect));
    }

    pub fn current_transform(&self) -> Transform2D {
        self.state.transform
    }

    pub fn current_opacity(&self) -> f32 {
        self.state.opacity
    }

    pub fn current_clip(&self) -> Option<Rect> {
        self.state.clip
    }

    // -- draw helpers ------------------------------------------------------

    pub fn fill_rect(&mut self, rect: Rect, paint: impl Into<Paint>) {
        self.commands.push(DrawCommand::FillRect {
            rect,
            paint: paint.into(),
        });
    }

    pub fn stroke_rect(&mut self, rect: Rect, width: f32, paint: impl Into<Paint>) {
        self.commands.push(DrawCommand::StrokeRect {
            rect,
            paint: paint.into(),
            width,
        });
    }

    /// Strokes a line segment from `from` to `to`, `width` logical pixels wide.
    pub fn draw_line(
        &mut self,
        from: draw_core::Vec2,
        to: draw_core::Vec2,
        width: f32,
        paint: impl Into<Paint>,
    ) {
        self.commands.push(DrawCommand::Line {
            from,
            to,
            paint: paint.into(),
            width,
        });
    }

    pub fn fill_circle(&mut self, center: Vec2, radius: f32, paint: impl Into<Paint>) {
        self.commands.push(DrawCommand::FillCircle {
            center,
            radius,
            paint: paint.into(),
        });
    }

    /// Fills a rounded rectangle with a uniform radius.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: f32, paint: impl Into<Paint>) {
        self.fill_rounded_rect_corners(rect, CornerRadii::uniform(radius), paint);
    }

    /// Fills a rounded rectangle with per-corner radii.
    pub fn fill_rounded_rect_corners(
        &mut self,
        rect: Rect,
        corners: CornerRadii,
        paint: impl Into<Paint>,
    ) {
        self.commands.push(DrawCommand::FillRoundedRect {
            rect,
            corners,
            paint: paint.into(),
        });
    }

    /// Strokes a rounded rectangle with a uniform outer radius.
    pub fn stroke_rounded_rect(
        &mut self,
        rect: Rect,
        radius: f32,
        width: f32,
        paint: impl Into<Paint>,
    ) {
        self.stroke_rounded_rect_corners(rect, CornerRadii::uniform(radius), width, paint);
    }

    /// Strokes a rounded rectangle with per-corner outer radii.
    pub fn stroke_rounded_rect_corners(
        &mut self,
        rect: Rect,
        corners: CornerRadii,
        width: f32,
        paint: impl Into<Paint>,
    ) {
        self.commands.push(DrawCommand::StrokeRoundedRect {
            rect,
            corners,
            paint: paint.into(),
            width,
        });
    }

    pub fn stroke_circle(
        &mut self,
        center: Vec2,
        radius: f32,
        width: f32,
        paint: impl Into<Paint>,
    ) {
        self.commands.push(DrawCommand::StrokeCircle {
            center,
            radius,
            paint: paint.into(),
            width,
        });
    }

    pub fn draw_image(
        &mut self,
        texture: TextureId,
        destination: Rect,
        source: Option<Rect>,
        paint: impl Into<Paint>,
    ) {
        self.commands.push(DrawCommand::DrawImage {
            texture,
            destination,
            source,
            paint: paint.into(),
        });
    }

    pub fn draw_text(
        &mut self,
        text: impl Into<String>,
        position: Vec2,
        font_size: f32,
        align: TextAlign,
        paint: impl Into<Paint>,
    ) {
        self.draw_text_weighted(text, position, font_size, FontWeight::NORMAL, align, paint);
    }

    /// [`draw_text`](Self::draw_text) with an explicit [`FontWeight`].
    pub fn draw_text_weighted(
        &mut self,
        text: impl Into<String>,
        position: Vec2,
        font_size: f32,
        weight: FontWeight,
        align: TextAlign,
        paint: impl Into<Paint>,
    ) {
        self.commands.push(DrawCommand::DrawText {
            text: text.into(),
            position,
            font_size,
            weight,
            align,
            paint: paint.into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Color, Size};

    fn rect(w: f32, h: f32) -> Rect {
        Rect::from_min_size(Vec2::ZERO, Size::new(w, h))
    }

    #[test]
    fn rounded_rect_helpers_record_commands() {
        let mut ctx = PaintContext::new();
        ctx.fill_rounded_rect(rect(40.0, 24.0), 6.0, Color::WHITE);
        ctx.stroke_rounded_rect(rect(40.0, 24.0), 6.0, 1.0, Color::BLACK);
        let list = ctx.into_draw_list();
        assert_eq!(list.len(), 2);
        assert!(matches!(
            list.commands()[0],
            DrawCommand::FillRoundedRect { corners, .. } if (corners.top_left - 6.0).abs() < 1e-5
        ));
        assert!(matches!(
            list.commands()[1],
            DrawCommand::StrokeRoundedRect { width, .. } if (width - 1.0).abs() < 1e-5
        ));
    }

    #[test]
    fn list_records_commands_in_order() {
        let mut ctx = PaintContext::new();
        ctx.fill_rect(rect(10.0, 10.0), Color::RED);
        ctx.stroke_rect(rect(10.0, 10.0), 2.0, Color::BLUE);
        let list = ctx.into_draw_list();
        assert_eq!(list.len(), 2);
        assert!(matches!(list.commands()[0], DrawCommand::FillRect { .. }));
        assert!(matches!(list.commands()[1], DrawCommand::StrokeRect { .. }));
    }

    #[test]
    fn draw_line_records_endpoints_width_and_paint() {
        let mut ctx = PaintContext::new();
        ctx.draw_line(Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0), 2.0, Color::BLUE);
        let list = ctx.into_draw_list();
        assert_eq!(
            list.commands(),
            &[DrawCommand::Line {
                from: Vec2::new(1.0, 2.0),
                to: Vec2::new(3.0, 4.0),
                paint: Paint::new(Color::BLUE),
                width: 2.0,
            }]
        );
    }

    #[test]
    fn save_restore_balances_and_recovers_state() {
        let mut ctx = PaintContext::new();
        ctx.set_opacity(0.5);
        ctx.save();
        ctx.set_opacity(0.1);
        assert_eq!(ctx.current_opacity(), 0.1);
        ctx.restore();
        assert_eq!(ctx.current_opacity(), 0.5);
        assert_eq!(ctx.depth(), 0);

        // unmatched restore is a silent no-op and emits nothing
        let before = ctx.len();
        ctx.restore();
        assert_eq!(ctx.len(), before);

        let list = ctx.into_draw_list();
        assert_eq!(
            list.commands(),
            &[
                DrawCommand::SetOpacity(0.5),
                DrawCommand::Save,
                DrawCommand::SetOpacity(0.1),
                DrawCommand::Restore,
            ]
        );
    }

    #[test]
    fn transform_is_absolute_and_composes() {
        let mut ctx = PaintContext::new();
        ctx.set_transform(Transform2D::from_translation(Vec2::new(10.0, 0.0)));
        ctx.transform(Transform2D::from_translation(Vec2::new(0.0, 5.0)));
        assert_eq!(ctx.current_transform().origin, Vec2::new(10.0, 5.0));
    }

    #[test]
    fn clip_state_is_tracked() {
        let mut ctx = PaintContext::new();
        assert_eq!(ctx.current_clip(), None);
        ctx.clip_rect(rect(100.0, 50.0));
        assert_eq!(ctx.current_clip(), Some(rect(100.0, 50.0)));
        ctx.save();
        ctx.clip_rect(rect(10.0, 10.0));
        assert_eq!(ctx.current_clip(), Some(rect(10.0, 10.0)));
        ctx.restore();
        assert_eq!(ctx.current_clip(), Some(rect(100.0, 50.0)));
    }

    #[test]
    fn drawing_is_deterministic() {
        fn build() -> DrawList {
            let mut ctx = PaintContext::new();
            ctx.fill_rect(rect(1.0, 2.0), Color::RED);
            ctx.fill_circle(Vec2::new(3.0, 4.0), 5.0, Color::GREEN);
            ctx.draw_text("hi", Vec2::ZERO, 12.0, TextAlign::Center, Color::WHITE);
            ctx.into_draw_list()
        }
        assert_eq!(build(), build());
    }
}
