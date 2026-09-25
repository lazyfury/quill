//! Sprite-sheet frame data: which atlas regions to show, and when.

use draw_core::{Rect, Size, Vec2};

/// A sprite-sheet animation's frame list plus its playback rate.
///
/// Pure data: [`SpriteFrames::region_at`] maps an elapsed time to a region, and
/// [`crate::SpriteAnimations`] applies that to a node each frame. A host that
/// drives the clock itself (for example a `draw_anim` playhead) can also call
/// `region_at` directly.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteFrames {
    frames: Vec<Rect>,
    fps: f32,
    looping: bool,
}

impl SpriteFrames {
    /// An animation from explicit atlas regions, at 12 fps, looping.
    pub fn from_regions(frames: impl IntoIterator<Item = Rect>) -> Self {
        Self {
            frames: frames.into_iter().collect(),
            fps: 12.0,
            looping: true,
        }
    }

    /// Slices `atlas` into a `columns` x `rows` grid and takes the first `count`
    /// frames in row-major order (a packed sprite sheet).
    pub fn from_grid(atlas: Rect, columns: u32, rows: u32, count: u32) -> Self {
        if columns == 0 || rows == 0 {
            return Self::from_regions(Vec::new());
        }
        let frame_size = Size::new(
            atlas.size.width / columns as f32,
            atlas.size.height / rows as f32,
        );
        let available = columns.saturating_mul(rows);
        let frames = (0..count.min(available)).map(|index| {
            let column = index % columns;
            let row = index / columns;
            Rect::from_min_size(
                Vec2::new(
                    atlas.left() + column as f32 * frame_size.width,
                    atlas.top() + row as f32 * frame_size.height,
                ),
                frame_size,
            )
        });
        Self::from_regions(frames)
    }

    /// Playback rate in frames per second (clamped to `>= 0`).
    pub fn fps(mut self, fps: f32) -> Self {
        self.fps = fps.max(0.0);
        self
    }

    /// Whether the animation wraps at the end (`false` holds the last frame).
    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Whether the animation loops.
    pub fn is_looping(&self) -> bool {
        self.looping
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// The region of frame `index`, if in range.
    pub fn frame(&self, index: usize) -> Option<Rect> {
        self.frames.get(index).copied()
    }

    /// Seconds for one full pass (0 with no frames or zero fps).
    pub fn duration(&self) -> f32 {
        if self.fps <= 0.0 {
            0.0
        } else {
            self.frames.len() as f32 / self.fps
        }
    }

    /// The frame index at `elapsed` seconds. Looping wraps; otherwise the last
    /// frame is held.
    pub fn index_at(&self, elapsed: f32) -> usize {
        if self.frames.is_empty() || self.fps <= 0.0 {
            return 0;
        }
        let raw = (elapsed.max(0.0) * self.fps).floor() as usize;
        if self.looping {
            raw % self.frames.len()
        } else {
            raw.min(self.frames.len() - 1)
        }
    }

    /// The region at `elapsed` seconds (`None` when there are no frames).
    pub fn region_at(&self, elapsed: f32) -> Option<Rect> {
        self.frame(self.index_at(elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(count: usize) -> SpriteFrames {
        let regions = (0..count).map(|index| {
            Rect::from_min_size(Vec2::new(index as f32 * 16.0, 0.0), Size::splat(16.0))
        });
        SpriteFrames::from_regions(regions).fps(4.0)
    }

    #[test]
    fn index_at_wraps_when_looping() {
        let frames = frames(4);
        assert_eq!(frames.index_at(0.0), 0);
        assert_eq!(frames.index_at(0.24), 0);
        assert_eq!(frames.index_at(0.25), 1);
        // 4 frames at 4 fps = 1 s per pass; 1.0 s wraps to frame 0.
        assert_eq!(frames.index_at(1.0), 0);
        assert_eq!(frames.index_at(1.1), 0);
    }

    #[test]
    fn index_at_holds_the_last_frame_when_not_looping() {
        let frames = frames(4).looping(false);
        assert_eq!(frames.duration(), 1.0);
        assert_eq!(frames.index_at(0.5), 2);
        assert_eq!(frames.index_at(1.0), 3);
        assert_eq!(frames.index_at(99.0), 3);
    }

    #[test]
    fn from_grid_slices_row_major() {
        let atlas = Rect::from_min_size(Vec2::ZERO, Size::new(64.0, 32.0));
        let frames = SpriteFrames::from_grid(atlas, 4, 2, 5);
        assert_eq!(frames.len(), 5);
        assert_eq!(
            frames.frame(0),
            Some(Rect::from_min_size(
                Vec2::new(0.0, 0.0),
                Size::new(16.0, 16.0)
            ))
        );
        // Frame 4 wraps to the second row, first column.
        assert_eq!(
            frames.frame(4),
            Some(Rect::from_min_size(
                Vec2::new(0.0, 16.0),
                Size::new(16.0, 16.0)
            ))
        );
    }

    #[test]
    fn zero_fps_has_no_duration_and_an_empty_set_is_safe() {
        assert_eq!(frames(3).fps(0.0).duration(), 0.0);
        assert_eq!(SpriteFrames::from_regions(Vec::new()).region_at(1.0), None);
    }
}
