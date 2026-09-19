use std::fmt;

/// A stable, generation-checked handle to a scene node.
///
/// The `index` identifies a storage slot; `generation` is bumped each time the
/// slot is recycled, so a stale `NodeId` never aliases a newer node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

impl NodeId {
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({}v{})", self.index, self.generation)
    }
}

/// Allocates and recycles [`NodeId`]s, bumping the generation on reuse.
#[derive(Debug, Clone, Default)]
pub struct NodeIdAllocator {
    generations: Vec<u32>,
    free: Vec<u32>,
}

impl NodeIdAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc(&mut self) -> NodeId {
        if let Some(index) = self.free.pop() {
            NodeId::new(index, self.generations[index as usize])
        } else {
            let index = self.generations.len() as u32;
            self.generations.push(0);
            NodeId::new(index, 0)
        }
    }

    /// Frees an id. Returns `false` if it is stale or out of range, in which
    /// case nothing changes.
    pub fn free(&mut self, id: NodeId) -> bool {
        let Some(generation) = self.generations.get_mut(id.index as usize) else {
            return false;
        };
        if *generation != id.generation {
            return false;
        }
        *generation = generation.wrapping_add(1);
        self.free.push(id.index);
        true
    }

    /// True when `id` matches the current generation of its slot.
    pub fn is_alive(&self, id: NodeId) -> bool {
        self.generations
            .get(id.index as usize)
            .is_some_and(|g| *g == id.generation)
    }

    /// Number of slots ever allocated (including freed ones).
    pub fn capacity(&self) -> usize {
        self.generations.len()
    }

    /// Number of currently-live slots.
    pub fn live_count(&self) -> usize {
        self.generations.len() - self.free.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_increments_index() {
        let mut alloc = NodeIdAllocator::new();
        let a = alloc.alloc();
        let b = alloc.alloc();
        assert_eq!(a.index(), 0);
        assert_eq!(b.index(), 1);
        assert_eq!(a.generation(), 0);
        assert!(alloc.is_alive(a));
        assert!(alloc.is_alive(b));
        assert_eq!(alloc.capacity(), 2);
        assert_eq!(alloc.live_count(), 2);
    }

    #[test]
    fn free_and_reuse_bumps_generation() {
        let mut alloc = NodeIdAllocator::new();
        let a = alloc.alloc();
        assert!(alloc.free(a));
        assert!(!alloc.is_alive(a));

        let b = alloc.alloc();
        assert_eq!(b.index(), a.index());
        assert_ne!(b.generation(), a.generation());
        assert!(alloc.is_alive(b));
        // stale id stays dead even though the index is reused
        assert!(!alloc.is_alive(a));
        assert_eq!(alloc.capacity(), 1);
        assert_eq!(alloc.live_count(), 1);
    }

    #[test]
    fn double_free_is_rejected() {
        let mut alloc = NodeIdAllocator::new();
        let a = alloc.alloc();
        assert!(alloc.free(a));
        assert!(!alloc.free(a));
        assert!(!alloc.is_alive(a));
    }

    #[test]
    fn ids_are_hashable_and_ordered_by_slot() {
        use std::collections::HashSet;
        let mut alloc = NodeIdAllocator::new();
        let a = alloc.alloc();
        let b = alloc.alloc();
        let mut set = HashSet::new();
        assert!(set.insert(a));
        assert!(set.insert(b));
        assert!(!set.insert(a));
        assert!(a < b);
    }
}
