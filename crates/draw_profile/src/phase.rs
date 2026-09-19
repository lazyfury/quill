/// A pipeline stage that can be timed independently.
///
/// The order of the variants matches the runtime order of the pipeline, so a
/// [`StageTimes`](crate::StageTimes) breakdown reads top-to-bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Application/component state update (dirty flags, animation, input).
    Update,
    /// `Control` anchors/offsets/containers resolved to absolute rects.
    Layout,
    /// Scene/UI emit `DrawCommand`s into a `DrawList`.
    Paint,
    /// A `RenderBackend` consumes the `DrawList` and produces output.
    Render,
}

impl Phase {
    /// Every phase, in pipeline order.
    pub const ALL: [Phase; 4] = [Phase::Update, Phase::Layout, Phase::Paint, Phase::Render];

    /// Stable index into a per-phase array (`0..4`).
    pub const fn index(self) -> usize {
        match self {
            Phase::Update => 0,
            Phase::Layout => 1,
            Phase::Paint => 2,
            Phase::Render => 3,
        }
    }

    /// Reverse of [`Phase::index`]; `None` for out-of-range indices.
    pub const fn from_index(index: usize) -> Option<Phase> {
        match index {
            0 => Some(Phase::Update),
            1 => Some(Phase::Layout),
            2 => Some(Phase::Paint),
            3 => Some(Phase::Render),
            _ => None,
        }
    }

    /// Short human-readable label (used by the debug overlay).
    pub const fn label(self) -> &'static str {
        match self {
            Phase::Update => "update",
            Phase::Layout => "layout",
            Phase::Paint => "paint",
            Phase::Render => "render",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_round_trips_and_is_stable() {
        for (i, phase) in Phase::ALL.iter().enumerate() {
            assert_eq!(phase.index(), i);
            assert_eq!(Phase::from_index(i), Some(*phase));
        }
        assert_eq!(Phase::from_index(Phase::ALL.len()), None);
        assert_eq!(Phase::from_index(usize::MAX), None);
    }

    #[test]
    fn labels_are_unique() {
        let mut labels: Vec<_> = Phase::ALL.iter().map(|phase| phase.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), Phase::ALL.len());
    }
}
