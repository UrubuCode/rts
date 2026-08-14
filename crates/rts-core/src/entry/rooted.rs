//! Values a native is accumulating, while it accumulates them.
//!
//! # The gap this closes, measured
//!
//! A native that builds a list of values holds it in a Rust `Vec<u64>` on its
//! own frame. [`super::roots::scan_stack`] walks the machine stack and keeps
//! words that are unambiguously encoded references — it never reaches the
//! `Vec`'s buffer, which is on the Rust heap and nowhere near that range. So
//! every value already in the list is invisible to a collection, and the
//! natives that build lists are exactly the ones that allocate while building
//! them.
//!
//! It is not a hypothetical. `xs.map(v => ({v}))` over 2 000 elements, 300
//! rounds, release, 2026-08-14: **nine rounds came back with wrong data** — not
//! a crash, an answer. The callback allocates, the allocation collects, and the
//! objects the map had already produced were swept out of a `Vec` nothing could
//! see.
//!
//! `super::array_proto::built` has the same hole one level down, and it is why
//! this is not fixed at the call sites one at a time: it asks `array_new` for
//! the array BEFORE it stores the values, so the values are unreachable for the
//! duration of that allocation. Reordering does not help — whichever comes
//! second, the other is invisible while it runs.
//!
//! # What already answers this, and why it is not enough
//!
//! [`super::external`] holds a value from outside the heap, and is the right
//! mechanism for what it was built for: ONE value, kept across calls, named by
//! an identifier a `.node` addon stores. It is wrong here twice over — a `map`
//! would hold and release one identifier per element, and `release` is a linear
//! scan, so a 2 000-element map would be quadratic.
//!
//! What is needed is O(1) for a whole list, which means registering the list
//! rather than its contents.
//!
//! # Why an address, and why a thread-local rather than the context
//!
//! Copying the accumulated values into the context at each step is safe Rust
//! and costs a borrow per element on the hottest list-building paths in the
//! runtime. Registering where the `Vec` LIVES costs one borrow per call —
//! except that it costs none, because the registry is not in the context at
//! all.
//!
//! It is a thread-local of its own for a reason that is a correctness one and
//! not a preference: the guard un-registers when it is DROPPED, and a native
//! regularly drops one inside `with_current`. If the registry lived on the
//! context, that drop would be a second `borrow_mut` of a `RefCell` already
//! borrowed — which aborts the process from an `extern "C"` frame, the exact
//! trap this crate's README names. `super::current::THROWN` is the established
//! precedent for state that must be reachable without the borrow.
//!
//! The `Vec` header is at a stable address for as long as the guard is alive:
//! it is boxed, so moving the guard does not move it, and growing the `Vec`
//! reallocates the buffer without moving the header — which is exactly why the
//! header is what gets registered.
//!
//! The `unsafe` is narrower than what this module's neighbour already does:
//! `scan_stack` reads arbitrary machine words and decides afterwards whether
//! they were references. This reads a typed pointer to a live local.

use std::cell::RefCell;

thread_local! {
    /// Where each list being built on this thread lives.
    ///
    /// Not on [`super::Context`] — see the module documentation for why that
    /// would abort — and not a `Cell<Vec<_>>`, because both registering and
    /// walking want the vector in place rather than taken and put back.
    static BUILDING: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// A list of values the collector must walk while a native builds it.
///
/// Registered on construction and removed on drop, so a native cannot hold an
/// unregistered list: the only way to get one is through [`Rooted::new`] or
/// [`Rooted::with`], and the only way to lose the registration is to drop the
/// guard, which drops the list with it.
pub(in crate::entry) struct Rooted {
    /// Where the values are. Boxed so the address belongs to this guard rather
    /// than to the frame that made it, which is what makes moving one sound.
    values: Box<Vec<u64>>,
}

impl Rooted {
    /// An empty list the collector can see.
    pub(in crate::entry) fn new() -> Self {
        Self::with(Vec::new())
    }

    /// The same, over values the caller already has.
    pub(in crate::entry) fn with(values: Vec<u64>) -> Self {
        let values = Box::new(values);
        let address = (&raw const *values) as usize;
        BUILDING.with(|held| held.borrow_mut().push(address));
        Rooted { values }
    }

    /// The list, to read and to add to.
    pub(in crate::entry) fn values(&mut self) -> &mut Vec<u64> {
        &mut self.values
    }

    /// The values, still registered, for a caller that only reads.
    pub(in crate::entry) fn as_slice(&self) -> &[u64] {
        &self.values
    }

    /// How many values are in it, without un-registering.
    pub(in crate::entry) fn len(&self) -> usize {
        self.values.len()
    }

    /// Hands the values over, un-registering as it does.
    ///
    /// Whoever takes them is responsible for what happens next, and for every
    /// caller here that means handing them straight to something that puts them
    /// where the collector reaches them, **with no allocation in between**.
    pub(in crate::entry) fn take(mut self) -> Vec<u64> {
        std::mem::take(&mut self.values)
    }
}

impl Drop for Rooted {
    fn drop(&mut self) {
        let address = (&raw const *self.values) as usize;
        BUILDING.with(|held| {
            let mut held = held.borrow_mut();
            // From the back: lists nest, and the innermost is the one ending.
            if let Some(at) = held.iter().rposition(|entry| *entry == address) {
                held.remove(at);
            }
        });
    }
}

/// Every value in every list being built on this thread right now.
///
/// # Safety
///
/// Each address was registered by a live [`Rooted`] and is removed by its
/// `Drop`, so every one of them points at a `Vec<u64>` that exists. The `Box`
/// is what keeps that true across a move of the guard.
pub(in crate::entry) fn building_roots() -> Vec<u64> {
    BUILDING.with(|held| {
        let mut found = Vec::new();
        for address in held.borrow().iter() {
            // SAFETY: see the module and type documentation — the address
            // belongs to a `Rooted` that has not been dropped, and a dropped
            // one removes its own entry before its `Vec` goes away.
            let values = unsafe { &*(*address as *const Vec<u64>) };
            found.extend_from_slice(values);
        }
        found
    })
}
