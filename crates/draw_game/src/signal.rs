//! Lightweight typed signals: a one-to-many callback list.
//!
//! Not an ECS and not a node system — just a small value a game can emit from
//! (for example `on_died`, `on_score`) and listeners can subscribe to. Cloning a
//! signal shares its handler list, so a node can own one and hand clones around.

use std::cell::RefCell;
use std::rc::Rc;

/// A typed, one-to-many callback list.
pub struct Signal<T> {
    handlers: Rc<RefCell<Vec<Box<dyn FnMut(&T)>>>>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            handlers: Rc::clone(&self.handlers),
        }
    }
}

impl<T> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Signal<T> {
    /// Creates an empty signal.
    pub fn new() -> Self {
        Self {
            handlers: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Adds a handler, called in registration order on every [`Signal::emit`].
    ///
    /// A handler must not call `connect` / `disconnect_all` / `emit` on the same
    /// signal (that would re-enter its `RefCell` and panic).
    pub fn connect(&self, handler: impl FnMut(&T) + 'static) {
        self.handlers.borrow_mut().push(Box::new(handler));
    }

    /// Calls every handler with `value`.
    pub fn emit(&self, value: &T) {
        for handler in self.handlers.borrow_mut().iter_mut() {
            handler(value);
        }
    }

    /// Number of connected handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.borrow().len()
    }

    /// Removes every handler.
    pub fn disconnect_all(&self) {
        self.handlers.borrow_mut().clear();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn emit_calls_every_handler_in_order() {
        let signal = Signal::<i32>::new();
        let first = Rc::new(Cell::new(0));
        let second = Rc::new(Cell::new(0));
        let (a, b) = (first.clone(), second.clone());

        signal.connect(move |value| a.set(*value));
        signal.connect(move |value| b.set(b.get() + *value));
        assert_eq!(signal.handler_count(), 2);

        signal.emit(&7);
        assert_eq!(first.get(), 7);
        assert_eq!(second.get(), 7);
    }

    #[test]
    fn clones_share_the_same_handler_list() {
        let signal = Signal::<i32>::new();
        let clone = signal.clone();
        let hits = Rc::new(Cell::new(0));
        let counter = hits.clone();

        clone.connect(move |_| counter.set(counter.get() + 1));
        assert_eq!(signal.handler_count(), 1);
        signal.emit(&0);
        assert_eq!(hits.get(), 1);
    }

    #[test]
    fn disconnect_all_clears_handlers() {
        let signal = Signal::<()>::new();
        signal.connect(|_| {});
        signal.disconnect_all();
        assert_eq!(signal.handler_count(), 0);
    }
}
