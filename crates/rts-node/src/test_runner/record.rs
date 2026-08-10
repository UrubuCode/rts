//! What `node:test` has reported so far.
//!
//! # Why this is a SECOND record, not the one `rts-std::test` already has
//!
//! `crates/rts-std/src/test.rs` already records `describe`/`test`/`expect`
//! results and `rts-host` already reads that record — the obvious question
//! before writing a second one is whether this should reuse it instead.
//!
//! `docs/reference/node/test.md` §4 answers this directly, for this module by
//! name: *"`node:test` must not depend on or reuse the `rts:test` harness's
//! native pieces — even though the two are conceptually similar… Flagged
//! again in §7 to prevent an implementer from reaching for the existing
//! `rts:test` internals as a shortcut."* That is not a preference this task
//! is free to override — it is the spec this module is built against, and it
//! is consistent with the architecture fact both docs restate: `rts-node`
//! is independent of `rts-std`/`rts-runtime` and must not gain a
//! dependency on either to get a native piece.
//!
//! `rts-std::test::record`/`reset` ARE already `pub fn`s — a public
//! accessor is not the blocker. A crate dependency is: `rts-node` has no
//! `rts-std` line in its `Cargo.toml` (not a file this task owns, and not
//! one the spec wants added for this). So this is a second record on purpose,
//! for a second, differently-imported module (`node:test` vs `rts:test`) —
//! not two numberings of one space. A program that imports ONE of the two
//! surfaces reports through that one; a program importing both would see two
//! independent tallies, which is the same divergence Node itself has no
//! occasion to produce (it has only ever had the one).

use std::cell::RefCell;

/// What one `test(…)` reported.
pub struct Reported {
    pub name: String,
    /// `None` is a pass. Set the first time the body throws in a way this
    /// module can observe (see the module doc on the exception-catching
    /// gap) or the body answers a thenable (refused async form).
    pub failure: Option<String>,
}

thread_local! {
    /// Thread-local for the same reason `rts-std::test`'s is: a context
    /// is per-thread in this engine, and `node:test`'s own spec (§5.4) makes
    /// this a documented REQUIREMENT, not just a style choice — a worker's
    /// suite must never interleave with another region's.
    static RECORD: RefCell<Vec<Reported>> = const { RefCell::new(Vec::new()) };
}

/// Appends one report, in declaration order.
pub fn push(name: String, failure: Option<String>) {
    RECORD.with(|held| held.borrow_mut().push(Reported { name, failure }));
}

/// Everything reported since the last [`reset`], in order.
pub fn record() -> Vec<Reported> {
    RECORD.with(|held| {
        held.borrow()
            .iter()
            .map(|one| Reported { name: one.name.clone(), failure: one.failure.clone() })
            .collect()
    })
}

/// Empties the record, for a host running more than one program.
pub fn reset() {
    RECORD.with(|held| held.borrow_mut().clear());
}
