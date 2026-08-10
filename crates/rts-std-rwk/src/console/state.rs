//! What `console.group`, `console.time` and `console.count` remember between
//! calls — indentation depth, a stopwatch per label, a counter per label.
//!
//! # Why thread-local rather than on `Context`
//!
//! None of it is a JavaScript value: a program cannot read its own indent depth
//! or reach a timer as a value, so putting it on `rts-core-rwk`'s `Context`
//! would ask that crate to carry state nothing in the language can observe. It
//! is process state a diagnostic stream keeps, the same shape `print`'s own
//! buffering is, and `console`'s Node specification agrees — a browser's
//! `console` keeps exactly this per **page**, not per script.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

thread_local! {
    static GROUP_DEPTH: RefCell<u32> = const { RefCell::new(0) };
    static TIMERS: RefCell<HashMap<String, Instant>> = RefCell::new(HashMap::new());
    static COUNTS: RefCell<HashMap<String, u32>> = RefCell::new(HashMap::new());
}

/// Two spaces per open `console.group`, prefixed on every line this module
/// prints — what Node's own `console` does, checked against it directly.
pub fn indent() -> String {
    GROUP_DEPTH.with(|depth| "  ".repeat(*depth.borrow() as usize))
}

pub fn push_group() {
    GROUP_DEPTH.with(|depth| *depth.borrow_mut() += 1);
}

pub fn pop_group() {
    GROUP_DEPTH.with(|depth| {
        let mut depth = depth.borrow_mut();
        *depth = depth.saturating_sub(1);
    });
}

pub fn start_timer(label: &str) {
    TIMERS.with(|timers| timers.borrow_mut().insert(label.to_owned(), Instant::now()));
}

/// The elapsed time since `start_timer(label)`, taking the timer — Node's own
/// `timeEnd` stops the stopwatch; `timeLog` would not, and this crate does not
/// distinguish the two, which is named in `console::mod`'s installation list
/// rather than hidden.
pub fn read_timer(label: &str) -> Option<Duration> {
    TIMERS.with(|timers| timers.borrow_mut().remove(label)).map(|start| start.elapsed())
}

/// One more call under `label`, answering the running total.
pub fn bump_count(label: &str) -> u32 {
    COUNTS.with(|counts| {
        let mut counts = counts.borrow_mut();
        let entry = counts.entry(label.to_owned()).or_insert(0);
        *entry += 1;
        *entry
    })
}

pub fn reset_count(label: &str) {
    COUNTS.with(|counts| {
        counts.borrow_mut().remove(label);
    });
}
