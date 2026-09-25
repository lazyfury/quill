use draw_core::ViewportSize;

use crate::list::DrawList;
use crate::texture::TextureId;

/// A backend that turns a [`DrawList`] into output for a frame.
///
/// Lifecycle per frame: [`begin_frame`](RenderBackend::begin_frame) ->
/// [`submit`](RenderBackend::submit) (zero or more times) ->
/// [`end_frame`](RenderBackend::end_frame). Backends must not assume `submit`
/// is called exactly once; a scene may be painted in multiple lists.
pub trait RenderBackend {
    /// Error produced by the backend. Use [`std::convert::Infallible`] for
    /// backends that cannot fail.
    type Error: core::fmt::Debug;

    /// Starts a frame targeting the given logical [`ViewportSize`].
    fn begin_frame(&mut self, viewport: ViewportSize) -> Result<(), Self::Error>;

    /// Submits one draw list for the current frame.
    fn submit(&mut self, list: &DrawList) -> Result<(), Self::Error>;

    /// Ends the current frame and presents/records it.
    fn end_frame(&mut self) -> Result<(), Self::Error>;

    /// Registers a decoded RGBA8 image so `DrawImage` can reference it by
    /// [`TextureId`].
    ///
    /// The neutral contract is bytes only: each backend maps the handle to its
    /// own resource (a GPU texture, an `ImageBitmap`, a recording). `rgba` must
    /// hold at least `width * height * 4` bytes, row-major, straight alpha.
    ///
    /// The default implementation ignores the texture and returns `Ok(())`, so a
    /// backend with no image support (or a headless one) needs no code; a caller
    /// must treat `Ok` from such a backend as "decoded but not displayed".
    ///
    /// **Non-breaking addition to `draw_render`** (Stage 28.2); recorded in
    /// `docs/design-system.md`.
    fn register_texture(
        &mut self,
        _id: TextureId,
        _width: u32,
        _height: u32,
        _rgba: &[u8],
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}
