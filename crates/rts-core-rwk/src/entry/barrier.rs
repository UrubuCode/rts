//! Telling the collector a reference was written.
//!
//! # What a barrier is for, in one sentence
//!
//! A collector that reclaims one region without looking at the others has to be
//! told when a cell **outside** the region it is collecting starts pointing
//! **into** it — otherwise it sees no reference to that object, frees it, and
//! the outside cell holds an address to nothing. The set of such stores is the
//! remembered set, and the barrier is what fills it.
//!
//! The direction of the error is chosen in the machine's lowering rather than
//! here, and the reasoning is worth repeating because it is why this runs on
//! every reference store rather than on the ones that look interesting:
//!
//! > A barrier that was not needed costs a call. A barrier that was needed and
//! > skipped produces a reference the collector never learns about, which
//! > becomes a use-after-free that reproduces rarely and explains nothing. The
//! > first is a bill; the second is a bug nobody can find.
//!
//! # Why this records something now, when it counted before
//!
//! It counted because there was one region: a store could not cross one, so
//! there was nothing to remember and the count existed only so the call site
//! would not have to be found again.
//!
//! There are now as many regions as a program was compiled for, one per thread,
//! and `rts_cranelift::gc::BarrierKind::CrossRegion` is the case the machine has
//! always modelled. So the barrier asks the question the machine named — does
//! this store make one region point at another — and records the answer.
//!
//! # What it should record today, and why that is worth building anyway
//!
//! **Nothing.** A thread can only reach references its own region handed out,
//! because nothing publishes a value from one thread to another — there is no
//! channel, no shared global, no way to pass a reference across. So the
//! remembered set is correct and empty.
//!
//! That is not a reason to leave it a counter. Two reasons:
//!
//! **It is the check that the isolation holds.** An entry appearing here means a
//! reference crossed a thread, which nothing is supposed to be able to do. It is
//! evidence of a defect somewhere else, and evidence is what a `debug_assert`
//! and a test can look at. A counter cannot say which store did it.
//!
//! **The collector needs it built, not planned.** When one arrives it consults
//! this; a barrier that had been counting would have to be written then, under
//! the pressure of a collector that does not work yet, which is the worst moment
//! to write the thing everything else depends on.
//!
//! # What this still does not do
//!
//! Nothing here makes publication safe. Recording that a reference crossed is
//! not the same as making the crossing sound: the object would also have to be
//! promoted out of a thread-local region before another thread could see it, and
//! `Region::Local` versus `Shared` is decided nowhere. This is the bookkeeping,
//! not the mechanism.

use super::{Context, with_current};
use crate::value::Value;

/// The stores that made one region point at another.
///
/// # Why a set of stores and not a set of objects
///
/// Because the collector's question is "which cells outside the region I am
/// collecting hold a reference into it", and a cell is what it has to re-scan.
/// Recording the written *value* instead would say what is still alive without
/// saying where from — and the collector needs the where, so that it can find
/// the reference again after moving what it points at.
#[derive(Default)]
pub struct Remembered {
    /// The cell that was written to, and the region the value it now holds
    /// belongs to.
    ///
    /// A `Vec` rather than a set: duplicates are possible — one cell written
    /// twice — and de-duplicating on every store would put a lookup on the
    /// barrier's path to save memory on a table that is empty in every program
    /// that exists today. A collector reading this sorts it once instead.
    crossings: Vec<(u32, u32)>,
}

impl Remembered {
    /// How many crossings were recorded.
    ///
    /// Zero in any program this engine can currently express, which is what
    /// makes it worth asserting rather than merely reporting.
    pub fn len(&self) -> usize {
        self.crossings.len()
    }

    /// Whether nothing crossed.
    pub fn is_empty(&self) -> bool {
        self.crossings.is_empty()
    }

    /// Every store that crossed, as the cell written and the region written.
    pub fn crossings(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.crossings.iter().copied()
    }
}

/// Records that `written` was stored into `object`.
///
/// Both are values rather than references: the store that called this may have
/// written a number, and a barrier that only ran for references would need the
/// caller to have decided which it was — which is the decision the barrier
/// exists to not depend on.
#[rtse::entry("rts_write_barrier")]
pub fn write_barrier(object: u64, written: u64) {
    with_current(|context| {
        context.barriers += 1;
        if let Some(crossing) = crossing(context, object, written) {
            context.remembered.crossings.push(crossing);
        }
    })
}

/// The crossing a store made, if it made one.
///
/// `None` for a store of anything that is not a reference — a number crosses
/// nothing — and for a reference into the region doing the storing, which is
/// every reference a thread can currently hold.
fn crossing(context: &Context, object: u64, written: u64) -> Option<(u32, u32)> {
    let target = Value(written).as_slot()?;
    let into = Value(object).as_slot()?;
    let here = context.region.index();
    let theirs = context.region.region_of(target);
    if theirs == here {
        return None;
    }
    // Reached only if a reference crossed a thread, which nothing can currently
    // do. Recorded rather than refused: the barrier's job is to tell the
    // collector, not to decide that the program is wrong — and a store the
    // engine cannot express is better seen in a test than turned into a crash
    // in a program.
    Some((into, theirs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::with_context;
    use crate::value::{Kinds, Singletons};

    /// A context over a region of a four-region heap.
    fn hosted<T>(region: u32, body: impl FnOnce() -> T) -> (Context, T) {
        let singletons = Singletons {
            undefined: 0,
            null: 1,
        };
        let heap = crate::heap::Region::sharded(64, region, 2);
        let context = Context::over(singletons, Kinds::in_declaration_order(), heap);
        with_context(context, body)
    }

    #[test]
    fn a_store_within_one_region_remembers_nothing() {
        // The case every program reaches, and the one that must not pay: a
        // thread only ever holds references its own region handed out.
        let (context, ()) = hosted(1, || {
            let object = crate::entry::object_new();
            let other = crate::entry::object_new();
            write_barrier(object, other);
            write_barrier(object, Value::from_f64(1.0).bits());
        });
        assert_eq!(context.barriers, 2, "both stores called the barrier");
        assert!(
            context.remembered.is_empty(),
            "a store inside one region crossed nothing"
        );
    }

    #[test]
    fn a_store_of_another_regions_reference_is_remembered() {
        // Not reachable from a program — nothing publishes a value across a
        // thread — so the reference is forged here. That is the point: this is
        // the check that says so, and it is the only place the crossing can be
        // seen at all.
        let (context, forged) = hosted(1, || {
            let object = crate::entry::object_new();
            // A cell of region 2, spelled the way region 2 would spell it.
            let elsewhere = Value::from_slot((7 << 2) | 2).bits();
            write_barrier(object, elsewhere);
            object
        });
        assert_eq!(context.remembered.len(), 1, "the crossing was recorded");
        let (into, region) = context.remembered.crossings().next().expect("one crossing");
        assert_eq!(
            into,
            Value(forged).as_slot().expect("an object"),
            "the cell written, which is what a collector re-scans"
        );
        assert_eq!(region, 2, "the region the value it now holds belongs to");
    }
}
