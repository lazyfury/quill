//! 资源句柄：文档和图层都不持有对方，只持有 id。

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// 单调递增的 id 分配器。进程内唯一即可，句柄本身不需要可预测。
fn next() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// 文档句柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(u64);

impl DocumentId {
    /// 分配一个新的文档 id。
    pub fn next() -> Self {
        Self(next())
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "doc#{}", self.0)
    }
}

/// 图层句柄。只在所属文档内有意义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(u64);

impl LayerId {
    /// 分配一个新的图层 id。
    pub fn next() -> Self {
        Self(next())
    }
}

impl fmt::Display for LayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "layer#{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_display_a_stable_prefix() {
        let a = LayerId::next();
        let b = LayerId::next();
        assert_ne!(a, b);
        assert!(a.to_string().starts_with("layer#"));
        assert!(DocumentId::next().to_string().starts_with("doc#"));
    }
}
