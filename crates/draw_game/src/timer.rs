//! Lightweight timers: fire a callback after a delay, optionally repeating.
//!
//! A host owns one [`Timers`] and calls [`Timers::update`] each frame;
//! [`Timers::is_animating`] is part of its `needs_frame` signal while a timer is
//! pending. Timers are engine-neutral (no node, no clock): the host supplies
//! `dt`, so the same code is testable headlessly.

/// Stable handle for a running timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerId(u64);

struct RunningTimer {
    id: TimerId,
    duration: f32,
    remaining: f32,
    repeat: bool,
    on_timeout: Box<dyn FnMut()>,
    finished: bool,
}

/// Owns and advances active timers.
#[derive(Default)]
pub struct Timers {
    running: Vec<RunningTimer>,
    next_id: u64,
}

impl Timers {
    pub fn new() -> Self {
        Self {
            running: Vec::new(),
            next_id: 1,
        }
    }

    /// Starts a timer that fires after `duration` seconds. A repeating timer
    /// resets itself each time; a one-shot timer is dropped after firing.
    ///
    /// A non-positive duration fires on the next [`Timers::update`].
    pub fn start(
        &mut self,
        duration: f32,
        repeat: bool,
        on_timeout: impl FnMut() + 'static,
    ) -> TimerId {
        let id = TimerId(self.next_id);
        self.next_id += 1;
        let duration = duration.max(0.0);
        self.running.push(RunningTimer {
            id,
            duration,
            remaining: duration,
            repeat,
            on_timeout: Box::new(on_timeout),
            finished: false,
        });
        id
    }

    /// Cancels a timer. Returns whether it was running.
    pub fn cancel(&mut self, id: TimerId) -> bool {
        let before = self.running.len();
        self.running.retain(|timer| timer.id != id);
        self.running.len() != before
    }

    /// Cancels every timer.
    pub fn clear(&mut self) {
        self.running.clear();
    }

    /// Number of pending timers.
    pub fn len(&self) -> usize {
        self.running.len()
    }

    pub fn is_empty(&self) -> bool {
        self.running.is_empty()
    }

    /// Whether any timer is still pending (part of a host's `needs_frame`).
    pub fn is_animating(&self) -> bool {
        !self.running.is_empty()
    }

    /// Advances every timer by `dt`, firing the ones that reach zero.
    ///
    /// A repeating timer catches up (so a large `dt` fires every elapsed
    /// period). Callbacks run after all timers have been advanced, so a
    /// callback sees a consistent set of remaining times.
    pub fn update(&mut self, dt: f32) {
        let dt = dt.max(0.0);
        let mut due: Vec<TimerId> = Vec::new();
        for timer in &mut self.running {
            timer.remaining -= dt;
            if timer.remaining <= 0.0 {
                due.push(timer.id);
            }
        }

        for id in due {
            let Some(timer) = self.running.iter_mut().find(|timer| timer.id == id) else {
                continue;
            };
            if timer.repeat && timer.duration > 0.0 {
                while timer.remaining <= 0.0 {
                    (timer.on_timeout)();
                    timer.remaining += timer.duration;
                }
            } else {
                (timer.on_timeout)();
                if timer.repeat {
                    // A zero-duration repeating timer fires once per update.
                    timer.remaining = 0.0;
                } else {
                    timer.finished = true;
                }
            }
        }
        self.running.retain(|timer| !timer.finished);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn a_one_shot_timer_fires_once() {
        let hits = Rc::new(Cell::new(0));
        let counter = hits.clone();
        let mut timers = Timers::new();
        timers.start(0.5, false, move || counter.set(counter.get() + 1));
        assert!(timers.is_animating());

        timers.update(0.3);
        assert_eq!(hits.get(), 0);
        timers.update(0.3);
        assert_eq!(hits.get(), 1);
        assert!(!timers.is_animating(), "a one-shot timer is dropped");
    }

    #[test]
    fn a_repeating_timer_fires_each_period() {
        let hits = Rc::new(Cell::new(0));
        let counter = hits.clone();
        let mut timers = Timers::new();
        timers.start(0.25, true, move || counter.set(counter.get() + 1));

        timers.update(0.6);
        assert_eq!(hits.get(), 2, "two periods elapsed");
        timers.update(0.5);
        assert_eq!(hits.get(), 4, "two more periods");
        assert!(timers.is_animating());
    }

    #[test]
    fn cancel_stops_a_timer() {
        let hits = Rc::new(Cell::new(0));
        let counter = hits.clone();
        let mut timers = Timers::new();
        let id = timers.start(1.0, false, move || counter.set(counter.get() + 1));
        assert!(timers.cancel(id));

        timers.update(2.0);
        assert_eq!(hits.get(), 0);
        assert!(!timers.is_animating());
        assert!(!timers.cancel(id));
    }

    #[test]
    fn a_zero_duration_timer_fires_on_the_next_update() {
        let hits = Rc::new(Cell::new(0));
        let counter = hits.clone();
        let mut timers = Timers::new();
        timers.start(0.0, false, move || counter.set(counter.get() + 1));
        timers.update(0.0);
        assert_eq!(hits.get(), 1);
    }
}
