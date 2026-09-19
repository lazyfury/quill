//! A minimal consuming [`RenderBackend`] that discards the frame.
//!
//! The recording backend stores a clone of every command, so benchmarking a
//! long loop with it would grow memory without bound. `SinkBackend` only walks
//! the command list and keeps a count, which measures the *submission* cost
//! without retaining anything — the closest CPU analogue to "hand this to the
//! renderer".

use core::convert::Infallible;

use draw_core::Viewport;
use draw_render::{DrawList, RenderBackend};

/// Counts submitted commands and frames, retaining nothing.
#[derive(Debug, Clone, Default)]
pub struct SinkBackend {
    commands: usize,
    frames: usize,
}

impl SinkBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Commands submitted during the most recent frame.
    pub fn commands(&self) -> usize {
        self.commands
    }

    /// Frames completed so far.
    pub fn frames(&self) -> usize {
        self.frames
    }
}

impl RenderBackend for SinkBackend {
    type Error = Infallible;

    fn begin_frame(&mut self, _viewport: Viewport) -> Result<(), Infallible> {
        self.commands = 0;
        Ok(())
    }

    fn submit(&mut self, list: &DrawList) -> Result<(), Infallible> {
        // Walk the list so the work under test is not optimized away, but keep
        // only a count so memory stays flat across millions of frames.
        self.commands += list.commands().len();
        draw_bench::black_box(self.commands);
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Infallible> {
        self.frames += 1;
        Ok(())
    }
}
