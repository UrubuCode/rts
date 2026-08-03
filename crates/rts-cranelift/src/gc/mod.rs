//! The collector's contract.
//!
//! Where a collection may happen, what must be findable when it does, and what a
//! store owes a collector that reclaims one region at a time. All three are
//! machine facts: they depend on frame layout and on what a value is, and on
//! nothing a client could tell us.
//!
//! # Why this is derived and not declared
//!
//! The engine this replaces finds roots by reading every word of the stack and
//! treating anything that resembles a reference as one. That is honest and it
//! works — an accidental match only keeps something alive one cycle longer — but
//! it cannot support a collector that relocates, it costs proportionally to
//! stack depth rather than to anything a function knows, and it cannot tell a
//! number that happens to look like a reference from one.
//!
//! The compiler underneath already supports the precise alternative: declaring
//! that a value is a reference makes it spilled and recorded wherever it is live
//! across a point that can collect. Two such declarations exist and this
//! repository calls neither. So the gap between the conservative scan and a
//! precise one is not a missing capability — it is an uncalled one, plus the
//! discipline of never forgetting to call it.
//!
//! Discipline that must hold at every allocation in every program will not hold.
//! So it is not a discipline here: [`describe_frames`] derives the root set from
//! liveness, and there is no entry point through which a client could report a
//! set of its own, correct or otherwise.
//!
//! # Precise where described, conservative per frame elsewhere
//!
//! A frame this layer compiled has a descriptor and is read exactly. A frame it
//! did not — anything compiled another way — has none, and must be scanned
//! conservatively over its own range rather than assumed empty. Precision that
//! under-approximates frees memory that is still in use, which is worse than the
//! scan it replaces. That fallback is also what lets frames from two engines
//! share one call stack, which is what makes a gradual migration possible at all.

mod barrier;
mod frame;
mod liveness;

pub use barrier::{BarrierKind, barrier_for, can_carry_reference};
pub use frame::{FrameDescriptor, FrameTable, ResumeLabel, RootSlot};
pub use liveness::{Liveness, live_after_each_inst};

use crate::ir::Function;
use crate::repr::Repr;

/// Derives every frame descriptor in a function.
///
/// A point that can collect gets a descriptor naming the values live across it
/// that the collector must find. "Across" is the operative word: a value the
/// point itself produces is not yet live before it, and a value nothing reads
/// afterwards need not survive it.
pub fn describe_frames(func: &Function) -> FrameTable {
    let liveness = Liveness::compute(func);
    let mut table = FrameTable::new();

    for (block_id, block) in func.blocks() {
        if !block.insts.iter().any(|&i| func.inst(i).is_some_and(|d| d.inst.is_safepoint())) {
            continue;
        }

        let after = live_after_each_inst(func, block_id, &liveness);
        for (position, &inst_id) in block.insts.iter().enumerate() {
            let Some(data) = func.inst(inst_id) else { continue };
            if !data.inst.is_safepoint() {
                continue;
            }

            let mut roots: Vec<_> = after[position]
                .iter()
                // A value the point itself produces does not exist before it, so
                // it cannot be live *across* it. Reporting it would ask the
                // collector to find a value that does not yet exist at the
                // moment it looks.
                .filter(|&&value| Some(value) != data.result)
                .filter(|&&value| func.repr_of(value).is_gc_relevant())
                .map(|&value| RootSlot { value, kind: reference_kind(func.repr_of(value)) })
                .collect();

            // Ordered so that two runs over the same function produce the same
            // table: a root set that depends on hash iteration order is one that
            // cannot be compared between builds or reviewed in a diff.
            roots.sort_by_key(|slot| slot.value);

            // The region is recorded on the same record as the roots. A value
            // spilled so the collector can find it and a value that must survive
            // cleanup are then never two answers to one question.
            table.push(FrameDescriptor {
                at: inst_id,
                roots,
                region: func.region_of(block_id),
                resume_label: None,
            });
        }
    }
    table
}

/// What a traced value points at, when that is known here.
///
/// The generic form reports nothing: it may hold a reference of any kind, and a
/// claim made here would be a guess the collector would then trust.
fn reference_kind(repr: Repr) -> Option<crate::repr::RefKind> {
    match repr {
        Repr::Ref(kind) => Some(kind),
        _ => None,
    }
}
