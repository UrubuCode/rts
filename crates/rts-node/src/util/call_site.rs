//! `util.getCallSite()` — the call stack as objects a program can read.
//!
//! # Why this exists at all, when half of what it answers is missing
//!
//! Because the half it CAN answer is what almost every caller uses. Measured
//! 2026-08-24 against Node's own suite, `getCallSite is not a function` killed
//! **178 files** — every one of them through `common.mustNotCall`, which reads
//! a call site only to put the name and position into a message that is printed
//! when the test has already failed.
//!
//! # What is real here, and what is ABSENT rather than zero
//!
//! `functionName` is real: it comes from `entry::call_frames`, the same
//! `context.callees` walk an `Error`'s `.stack` is rendered from.
//!
//! `lineNumber`, `columnNumber` and `scriptName` are **not present on the
//! object**. They are not zero and not the empty string, and that is the whole
//! point: this engine records a source position per instruction and nothing
//! maps an address back to one at run time (`rts_cranelift::observe`'s
//! question), so any number here would be a line a program could act on and be
//! wrong about. A property that is absent answers `undefined`, which is a
//! program asking "where?" and being told "nobody knows" — the honest answer.
//!
//! That is this repository's rule applied rather than bent: *"a surface that
//! cannot do what its name means does not ship"*. What the name means is "give
//! me the call sites", and the call sites are given. What is missing is stated
//! by its absence at the property that would carry it, at the moment a program
//! reads it — not hidden behind a plausible number.
//!
//! # Both spellings
//!
//! `getCallSite` is Node 22's name and `getCallSites` is Node 23's, for the
//! same operation. Both are bound to this, because a corpus spans versions and
//! answering one spelling while refusing the other would report a Node version
//! difference as a missing feature.

use rts_core::entry::{self, Provided};

/// The members this module contributes to `node:util`.
pub(super) const MEMBERS: &[(&str, Provided)] = &[
    ("getCallSite", get_call_site),
    ("getCallSites", get_call_site),
];

/// `util.getCallSite([frames][, options])`.
///
/// The first argument is a frame COUNT in Node, and it caps the answer. Read as
/// a number and ignored when it is not one — Node's other overload takes an
/// options object there, and a count read out of an object would truncate the
/// answer to nothing.
extern "C" fn get_call_site(_e: u64, _this: u64, limit: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // Read OUTSIDE the borrow: `call_frames` takes one of its own, and a second
    // borrow of the same cell from inside is the re-entrancy that aborts a
    // process rather than unwinding.
    // Nothing is dropped, and the INDEX is why. `[0]` is this native itself —
    // unnamed, so it arrives as the empty string — and `[1]` is the caller,
    // which is exactly Node's numbering: `common.mustNotCall` reads
    // `getCallSite()[1]` to name the line that called it.
    //
    // Two earlier versions of this line were wrong in opposite directions. One
    // dropped `[0]` to "skip ourselves" and hid the caller; the other relied on
    // unnamed frames being filtered out, which made a call from a module's
    // top-level body — itself unnamed — answer a one-element list where `[1]`
    // was `undefined`, and 18 files died reading a property of it.
    let mut frames = entry::call_frames();
    if let Some(count) = entry::number_of(limit).filter(|held| *held >= 0.0) {
        frames.truncate(count as usize);
    }
    entry::with_runtime(|context| {
        let sites = frames
            .iter()
            .map(|name| {
                let site = entry::make_object(context);
                let held = entry::make_string(context, name);
                entry::put_member(context, site, "functionName", held);
                // `scriptName`, `lineNumber` and `columnNumber` are deliberately
                // NOT written. See this module's header: an absent property
                // answers `undefined`, and a number would be a lie a program
                // could act on.
                site
            })
            .collect();
        entry::make_array_in(context, sites)
    })
}
