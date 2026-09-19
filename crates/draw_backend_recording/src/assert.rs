use draw_core::{Rect, Transform2D};
use draw_render::DrawCommand;

/// Assertion helpers over a recorded command sequence.
///
/// Implemented for `[DrawCommand]`, so it applies to
/// `DrawList::commands()` and `RecordedFrame::commands()`.
pub trait CommandAsserts {
    fn command_count(&self) -> usize;
    fn has_command(&self, command: &DrawCommand) -> bool;
    fn count_matching<F: Fn(&DrawCommand) -> bool>(&self, predicate: F) -> usize;

    /// The most recent `SetTransform` value, if any.
    fn last_transform(&self) -> Option<Transform2D>;
    /// The most recent `SetOpacity` value, if any.
    fn last_opacity(&self) -> Option<f32>;
    /// The most recent `ClipRect` value, if any.
    fn last_clip(&self) -> Option<Rect>;

    fn assert_command_count(&self, expected: usize);
    fn assert_contains(&self, command: &DrawCommand);
    fn assert_not_contains(&self, command: &DrawCommand);
    fn assert_sequence(&self, expected: &[DrawCommand]);
    fn assert_last_transform(&self, expected: Transform2D);
    fn assert_last_opacity(&self, expected: f32);
    fn assert_last_clip(&self, expected: Rect);
}

impl CommandAsserts for [DrawCommand] {
    fn command_count(&self) -> usize {
        self.len()
    }

    fn has_command(&self, command: &DrawCommand) -> bool {
        self.contains(command)
    }

    fn count_matching<F: Fn(&DrawCommand) -> bool>(&self, predicate: F) -> usize {
        self.iter().filter(|command| predicate(command)).count()
    }

    fn last_transform(&self) -> Option<Transform2D> {
        self.iter().rev().find_map(|command| match command {
            DrawCommand::SetTransform(transform) => Some(*transform),
            _ => None,
        })
    }

    fn last_opacity(&self) -> Option<f32> {
        self.iter().rev().find_map(|command| match command {
            DrawCommand::SetOpacity(opacity) => Some(*opacity),
            _ => None,
        })
    }

    fn last_clip(&self) -> Option<Rect> {
        self.iter().rev().find_map(|command| match command {
            DrawCommand::ClipRect(rect) => Some(*rect),
            _ => None,
        })
    }

    fn assert_command_count(&self, expected: usize) {
        assert_eq!(
            self.command_count(),
            expected,
            "expected {expected} commands, found {:?}",
            self
        );
    }

    fn assert_contains(&self, command: &DrawCommand) {
        assert!(
            self.has_command(command),
            "expected command {command:?} in sequence {self:?}"
        );
    }

    fn assert_not_contains(&self, command: &DrawCommand) {
        assert!(
            !self.has_command(command),
            "did not expect command {command:?} in sequence {self:?}"
        );
    }

    fn assert_sequence(&self, expected: &[DrawCommand]) {
        assert_eq!(self, expected, "command sequence mismatch");
    }

    fn assert_last_transform(&self, expected: Transform2D) {
        assert_eq!(
            self.last_transform(),
            Some(expected),
            "unexpected last transform in sequence {self:?}"
        );
    }

    fn assert_last_opacity(&self, expected: f32) {
        assert_eq!(
            self.last_opacity(),
            Some(expected),
            "unexpected last opacity in sequence {self:?}"
        );
    }

    fn assert_last_clip(&self, expected: Rect) {
        assert_eq!(
            self.last_clip(),
            Some(expected),
            "unexpected last clip in sequence {self:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Color, Size, Vec2};
    use draw_render::{Paint, PaintContext};

    fn rect(w: f32, h: f32) -> Rect {
        Rect::from_min_size(Vec2::ZERO, Size::new(w, h))
    }

    #[test]
    fn helpers_query_recorded_commands() {
        let mut ctx = PaintContext::new();
        ctx.set_transform(Transform2D::from_translation(Vec2::new(3.0, 4.0)));
        ctx.set_opacity(0.5);
        ctx.clip_rect(rect(10.0, 10.0));
        ctx.fill_rect(rect(1.0, 1.0), Color::RED);
        let list = ctx.into_draw_list();
        let commands = list.commands();

        commands.assert_command_count(4);
        assert_eq!(
            commands.count_matching(|c| matches!(c, DrawCommand::FillRect { .. })),
            1
        );
        commands.assert_contains(&DrawCommand::FillRect {
            rect: rect(1.0, 1.0),
            paint: Paint::new(Color::RED),
        });
        commands.assert_last_transform(Transform2D::from_translation(Vec2::new(3.0, 4.0)));
        commands.assert_last_opacity(0.5);
        commands.assert_last_clip(rect(10.0, 10.0));
    }

    #[test]
    #[should_panic(expected = "command sequence mismatch")]
    fn assert_sequence_detects_mismatch() {
        let mut ctx = PaintContext::new();
        ctx.fill_rect(rect(1.0, 1.0), Color::RED);
        ctx.into_draw_list()
            .commands()
            .assert_sequence(&[DrawCommand::Save]);
    }
}
