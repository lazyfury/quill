use std::fmt;

use draw_core::Viewport;
use draw_render::{DrawCommand, DrawList, RenderBackend};

/// Errors from the frame lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingError {
    /// `begin_frame` was called while a frame is already open.
    AlreadyRecording,
    /// `submit`/`end_frame` was called with no open frame.
    NotRecording,
}

impl fmt::Display for RecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRecording => f.write_str("already recording a frame"),
            Self::NotRecording => f.write_str("no frame is currently recording"),
        }
    }
}

impl std::error::Error for RecordingError {}

/// One recorded frame: its viewport plus the concatenated commands of every
/// submitted [`DrawList`].
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedFrame {
    pub viewport: Viewport,
    pub draw_list: DrawList,
}

impl RecordedFrame {
    pub fn commands(&self) -> &[DrawCommand] {
        self.draw_list.commands()
    }

    pub fn command_count(&self) -> usize {
        self.draw_list.len()
    }
}

/// A headless [`RenderBackend`] that records frames for assertions.
///
/// This is the backbone of the no-browser test pipeline:
/// `Scene -> DrawList -> RecordingBackend -> assertions`.
#[derive(Debug, Clone, Default)]
pub struct RecordingBackend {
    frames: Vec<RecordedFrame>,
    current: Option<RecordedFrame>,
}

impl RecordingBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// All completed frames, oldest first.
    pub fn frames(&self) -> &[RecordedFrame] {
        &self.frames
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn frame(&self, index: usize) -> Option<&RecordedFrame> {
        self.frames.get(index)
    }

    pub fn last_frame(&self) -> Option<&RecordedFrame> {
        self.frames.last()
    }

    pub fn is_recording(&self) -> bool {
        self.current.is_some()
    }

    /// Discards all completed frames (does not touch an open frame).
    pub fn clear(&mut self) {
        self.frames.clear();
    }
}

impl RenderBackend for RecordingBackend {
    type Error = RecordingError;

    fn begin_frame(&mut self, viewport: Viewport) -> Result<(), Self::Error> {
        if self.current.is_some() {
            return Err(RecordingError::AlreadyRecording);
        }
        self.current = Some(RecordedFrame {
            viewport,
            draw_list: DrawList::new(),
        });
        Ok(())
    }

    fn submit(&mut self, list: &DrawList) -> Result<(), Self::Error> {
        let Some(frame) = self.current.as_mut() else {
            return Err(RecordingError::NotRecording);
        };
        for command in list.commands() {
            frame.draw_list.push(command.clone());
        }
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Self::Error> {
        let Some(frame) = self.current.take() else {
            return Err(RecordingError::NotRecording);
        };
        self.frames.push(frame);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Color, Rect, Size};
    use draw_render::PaintContext;

    fn viewport() -> Viewport {
        Viewport::new(Size::new(100.0, 100.0))
    }

    #[test]
    fn lifecycle_records_frames() {
        let mut backend = RecordingBackend::new();
        assert!(!backend.is_recording());

        backend.begin_frame(viewport()).unwrap();
        assert!(backend.is_recording());
        // nested begin is rejected
        assert_eq!(
            backend.begin_frame(viewport()),
            Err(RecordingError::AlreadyRecording)
        );

        let mut ctx = PaintContext::new();
        ctx.fill_rect(
            Rect::from_min_size(draw_core::Vec2::ZERO, Size::splat(10.0)),
            Color::RED,
        );
        backend.submit(&ctx.into_draw_list()).unwrap();

        backend.end_frame().unwrap();
        assert!(!backend.is_recording());
        assert_eq!(backend.frame_count(), 1);
        assert_eq!(backend.last_frame().unwrap().viewport, viewport());
        assert_eq!(backend.last_frame().unwrap().command_count(), 1);
    }

    #[test]
    fn submit_and_end_without_begin_fail() {
        let mut backend = RecordingBackend::new();
        assert_eq!(
            backend.submit(&DrawList::new()),
            Err(RecordingError::NotRecording)
        );
        assert_eq!(backend.end_frame(), Err(RecordingError::NotRecording));
    }

    #[test]
    fn multiple_submits_concatenate() {
        let mut backend = RecordingBackend::new();
        backend.begin_frame(viewport()).unwrap();

        let mut a = PaintContext::new();
        a.fill_rect(
            Rect::from_min_size(draw_core::Vec2::ZERO, Size::splat(1.0)),
            Color::RED,
        );
        let mut b = PaintContext::new();
        b.fill_circle(draw_core::Vec2::ZERO, 1.0, Color::BLUE);

        backend.submit(&a.into_draw_list()).unwrap();
        backend.submit(&b.into_draw_list()).unwrap();
        backend.end_frame().unwrap();

        assert_eq!(backend.last_frame().unwrap().command_count(), 2);
    }
}
