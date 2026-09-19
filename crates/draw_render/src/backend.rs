use draw_core::ViewportSize;

use crate::list::DrawList;

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
}
