use std::fmt;

use draw_core::ViewportSize;
use draw_render::{DrawCommand, DrawList, RenderBackend, TextureId};

/// Errors from the frame lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingError {
    /// `begin_frame` was called while a frame is already open.
    AlreadyRecording,
    /// `submit`/`end_frame` was called with no open frame.
    NotRecording,
    /// `register_texture` got a zero size or too few bytes.
    InvalidTexture,
}

impl fmt::Display for RecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRecording => f.write_str("already recording a frame"),
            Self::NotRecording => f.write_str("no frame is currently recording"),
            Self::InvalidTexture => f.write_str("invalid texture dimensions or byte length"),
        }
    }
}

impl std::error::Error for RecordingError {}

/// Metadata for a texture registered through the neutral contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisteredTexture {
    pub id: TextureId,
    pub width: u32,
    pub height: u32,
}

/// One recorded frame: its viewport plus the concatenated commands of every
/// submitted [`DrawList`].
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedFrame {
    pub viewport: ViewportSize,
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
    textures: Vec<RegisteredTexture>,
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

    /// Textures registered through [`RenderBackend::register_texture`], in
    /// registration order.
    pub fn textures(&self) -> &[RegisteredTexture] {
        &self.textures
    }

    /// Metadata for one registered texture.
    pub fn texture(&self, id: TextureId) -> Option<RegisteredTexture> {
        self.textures
            .iter()
            .copied()
            .find(|texture| texture.id == id)
    }

    /// Discards all completed frames (does not touch an open frame).
    pub fn clear(&mut self) {
        self.frames.clear();
    }
}

impl RenderBackend for RecordingBackend {
    type Error = RecordingError;

    fn begin_frame(&mut self, viewport: ViewportSize) -> Result<(), Self::Error> {
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

    fn register_texture(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), Self::Error> {
        let expected = width as usize * height as usize * 4;
        if width == 0 || height == 0 || rgba.len() < expected {
            return Err(RecordingError::InvalidTexture);
        }
        let registered = RegisteredTexture { id, width, height };
        match self.textures.iter_mut().find(|texture| texture.id == id) {
            Some(slot) => *slot = registered,
            None => self.textures.push(registered),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Color, Rect, Size};
    use draw_render::PaintContext;

    fn viewport() -> ViewportSize {
        ViewportSize::new(Size::new(100.0, 100.0))
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

    #[test]
    fn register_texture_records_metadata_and_replaces_the_same_id() {
        let mut backend = RecordingBackend::new();
        assert!(backend.textures().is_empty());

        let id = TextureId::new(5);
        backend.register_texture(id, 2, 2, &[0u8; 16]).unwrap();
        assert_eq!(
            backend.texture(id),
            Some(RegisteredTexture {
                id,
                width: 2,
                height: 2
            })
        );

        // Re-registering the same id replaces its metadata in place.
        backend.register_texture(id, 4, 2, &[0u8; 32]).unwrap();
        assert_eq!(backend.textures().len(), 1);
        assert_eq!(backend.texture(id).unwrap().width, 4);
    }

    #[test]
    fn register_texture_rejects_bad_dimensions_or_bytes() {
        let mut backend = RecordingBackend::new();
        assert_eq!(
            backend.register_texture(TextureId::new(1), 0, 2, &[]),
            Err(RecordingError::InvalidTexture)
        );
        assert_eq!(
            backend.register_texture(TextureId::new(1), 2, 2, &[0u8; 3]),
            Err(RecordingError::InvalidTexture)
        );
        assert!(backend.textures().is_empty());
    }
}
