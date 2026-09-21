//! Suspension: parking a frame and resuming it.
//!
//! Suspending and resuming a call frame is a machine capability, not a language
//! feature, and every client of interest needs it — coroutines in one,
//! generators and asynchronous functions in another. Owning it here is what makes
//! those the same feature rather than three implementations of varying honesty.
//!
//! # Why the frame is transformed rather than the stack switched
//!
//! There are two ways to suspend: transform the function so its live state lives
//! in an explicit record, or give it a stack of its own and switch stacks. The
//! second is not available — the code generator's stack-switching instruction is
//! implemented for one target, and it is not the one this project develops on.
//!
//! Availability aside, a switched stack introduces a second frame representation
//! that root reporting and unwinding would both have to understand, in a design
//! whose premise is that those two and suspension share one representation.
//!
//! The comparison settles it independently. The runtime that switches stacks owns
//! its entire backend and calling convention precisely so it can, and rewrites
//! references whenever a stack moves. The two implementations closest to this
//! situation — a portable scripting runtime and a production JavaScript engine —
//! both transform frames instead, because a compiled frame is not relocatable and
//! making it so costs more than flattening it.
//!
//! # What changes from the arrangement this replaces
//!
//! Not the mechanism: spilling live locals into a record with a resume position
//! is what the current engine's generators already do. What changes is *where* it
//! happens. Today the transformation is performed by one language's parser and
//! the code generator merely recognizes a protocol of calls, so a second language
//! reaching for coroutines would reproduce the entire transformation in its own
//! front end. Here it is driven by the same liveness the root reporting already
//! computes, over a record sized per function rather than shaped for one
//! language's generators.
//!
//! # Why this shares the frame descriptor
//!
//! A parked frame occupies no position on any call stack, and is still reasoned
//! about as a frame: its roots are read out of its record using the same records
//! root reporting uses, and the protected region it suspended inside is preserved
//! so that resuming re-establishes the correct cleanup chain. That is the whole
//! reason [`crate::gc::FrameDescriptor`] carries a region and a resume label
//! rather than three tables carrying one each.

mod layout;
mod spill;
mod transform;

pub use layout::FrameLayout;
pub use spill::{SpillLayout, SpillSlot};
pub use transform::{Resumable, TransformError, resumable_form, resumable_form_with_arrivals};

use crate::gc::{Liveness, live_after_each_inst};
use crate::ir::{Function, InstId};
use crate::repr::Repr;

/// Identifies a point a suspended frame can resume at.
///
/// Absent on a program point that can collect but is not a resumption target.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct ResumeLabel(pub u32);

/// How a parked frame is being picked back up.
///
/// Written by whoever resumes into [`FrameLayout::mode_field`], read by the
/// frame's own dispatch at the point it parked. Three ways, because a park is a
/// point control LEFT through and every way of leaving a point exists here
/// already: carrying on with a value, unwinding, and returning.
///
/// # Why this is the machine's and not a client's
///
/// The alternative was an operation the client emits after every suspension,
/// asking a runtime how this resumption was made. It puts the same question at
/// every suspension point in every client and gets it right only where somebody
/// remembered to ask — rule 8, exactly: a discipline that must hold at every
/// suspension in every program will not hold. The rewrite already owns the
/// resume point and already writes the dispatch that enters it, so the three
/// ways cost one compare each, on a path that has just re-entered a frame.
///
/// Nothing here names a source construct. A resumption that unwinds is a throw
/// in one language, a cancellation in another and a `panic` in a third; this
/// layer only knows that control leaves through the region tree, which is what
/// it knows about every other unwind.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash, PartialOrd, Ord)]
pub enum ResumeMode {
    /// Carry on from the suspension, with the delivered value as its result.
    ///
    /// Zero, so that a frame whose record was handed out zeroed resumes the
    /// ordinary way without anyone having written anything.
    #[default]
    Deliver,
    /// Leave through the region tree, with the delivered value as the payload.
    ///
    /// The tag is the one the client named when it asked for the rewrite. The
    /// throw happens AT the suspension point, so it is inside whatever
    /// protected regions the suspension was inside — which is the whole reason
    /// this is a mode of resuming rather than something the resumer could do
    /// on its own from outside.
    Unwind,
    /// Return from the function, running what leaving those regions owes.
    ///
    /// The delivered value is not stored where the function's answer goes: the
    /// resumer named it and therefore already holds it, and the frame's return
    /// slot need not even have the same representation. What the frame owes is
    /// that everything between the suspension and the exit runs.
    Return,
}

impl ResumeMode {
    /// The number written into the frame's record.
    ///
    /// One derivation, shared by the dispatch this crate emits and by the
    /// runtime that writes the field, because two spellings of one numbering is
    /// a frame that resumes one way and is asked for another.
    pub fn number(self) -> u64 {
        match self {
            ResumeMode::Deliver => 0,
            ResumeMode::Unwind => 1,
            ResumeMode::Return => 2,
        }
    }
}

/// Why a point in a frame exists.
///
/// # Why two kinds and not one
///
/// Because a suspension is two halves and a deoptimisation target is only the second
/// of them. Parking writes the label and leaves; resuming enters at the label with the
/// frame's contents in place. A guard that fails needs the entering half and must not
/// have the leaving half: the body it enters runs straight through when nothing
/// speculated, and a park emitted there would stop it at the very point a normal run
/// passes through.
///
/// Found by trying to reuse [`crate::frame::resumable_form`] for a side exit, which
/// `docs/engine/deopt-lateral.md` D3 assumed would fit. It does not fit and this is
/// exactly where -- which is what that section's own sentence asked for: if something
/// does not fit, the change is in `frame/` rather than a second implementation of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Entered {
    /// Control leaves here and comes back. Parks, and resumes.
    Suspension,
    /// Control only ARRIVES here, never leaves through it.
    ///
    /// The half a deoptimisation needs. Its live values are still preserved -- that is
    /// what makes arriving possible at all, and it is derived from liveness like every
    /// other spill rather than declared, which is rule 8.
    Arrival,
}

/// One point a frame can be entered at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Point {
    /// The instruction it is at. A suspension is AT its own instruction; an arrival is
    /// immediately before the one named, because that is where control lands.
    pub at: InstId,
    /// The number the dispatch selects it by.
    pub label: ResumeLabel,
    /// Which half of a suspension this is.
    pub entered: Entered,
}

/// How a function suspends.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SuspendPlan {
    /// Each point and the label control returns to.
    ///
    /// In program order, and the label is the position in that order: a
    /// resumption is a jump selected by a number, so the number is the order.
    pub points: Vec<Point>,
    /// What the frame preserves while it is parked.
    pub spill: SpillLayout,
}

impl SuspendPlan {
    /// Whether the function suspends at all.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// The label for a point, if this instruction is one.
    pub fn label_of(&self, at: InstId) -> Option<ResumeLabel> {
        self.points
            .iter()
            .find(|held| held.at == at)
            .map(|held| held.label)
    }

    /// Which half this instruction is, if it is a point at all.
    pub fn entered_at(&self, at: InstId) -> Option<Entered> {
        self.points
            .iter()
            .find(|held| held.at == at)
            .map(|held| held.entered)
    }
}

/// Derives how a function suspends.
///
/// Every value live across a suspension is preserved — not only the ones the
/// collector cares about. A number that survives a suspension is as necessary to
/// resuming correctly as a reference is, and asking a client to distinguish them
/// is asking it to get the distinction wrong somewhere.
///
/// As with root reporting, the point's own result is excluded: it is the value
/// delivered *by* resuming, so it does not exist to be preserved.
pub fn plan_suspension(func: &Function) -> SuspendPlan {
    plan_suspension_with(func, &Liveness::compute(func))
}

/// Derives how a function suspends, reusing liveness already computed.
///
/// Root reporting needs the same analysis, and computing it twice for one
/// function is work whose only cause would be module boundaries.
pub fn plan_suspension_with(func: &Function, liveness: &Liveness) -> SuspendPlan {
    plan_with_arrivals(func, liveness, &[])
}

/// The same, plus points control may ARRIVE at without ever leaving through them.
///
/// # What an arrival is for
///
/// A deoptimisation target. A guard in one tier fails, and the other tier has to be
/// entered at the corresponding place with the state that tier had -- which is the
/// entering half of a suspension and none of the leaving half.
///
/// # Why the arrivals are NAMED and the spill is not
///
/// Which points exist is a decision belonging to whoever emits the guards: this layer
/// cannot know where a client chose to speculate. What must be PRESERVED at one is not
/// a decision at all -- it is the live set, derived here from the same liveness a
/// suspension uses, because rule 8 says a client that could forget a value would.
///
/// So the signature takes the where and never the what.
///
/// An arrival names the instruction control lands ON, and that instruction runs after
/// the frame is entered. Naming the one before it would make an empty block the
/// target whenever a point sat at a block's start.
pub fn plan_with_arrivals(
    func: &Function,
    liveness: &Liveness,
    arrivals: &[InstId],
) -> SuspendPlan {
    let mut points = Vec::new();
    let mut preserved: Vec<(crate::ir::ValueId, Repr)> = Vec::new();

    for (block_id, block) in func.blocks() {
        // A BLOCK WITH NEITHER IS SKIPPED, and the arrival half of this test is what the
        // first run of it was missing: the filter asked only about suspensions, so a
        // block holding an arrival and nothing else was walked past and the point was
        // never planned. `resume_points` came out 0 and the test that ran the body
        // passed for having nothing to park at -- a false green that the other test
        // caught by counting.
        let interesting = block
            .insts
            .iter()
            .any(|&i| func.inst(i).is_some_and(|d| d.inst.is_suspend()) || arrivals.contains(&i));
        if !interesting {
            continue;
        }

        let after = live_after_each_inst(func, block_id, liveness);
        for (position, &inst_id) in block.insts.iter().enumerate() {
            let Some(data) = func.inst(inst_id) else {
                continue;
            };
            let entered = match data.inst.is_suspend() {
                true => Entered::Suspension,
                false if arrivals.contains(&inst_id) => Entered::Arrival,
                false => continue,
            };

            points.push(Point {
                at: inst_id,
                label: ResumeLabel(points.len() as u32),
                entered,
            });
            // WHAT IS LIVE AFTER A SUSPENSION and what is live BEFORE an arrival are
            // different sets, and the difference is the whole of why this is not one
            // line. A suspension comes back to just AFTER itself, so what it needs is
            // what outlives it. An arrival lands ON its instruction, which has not run
            // yet, so what it needs includes that instruction's own operands.
            match entered {
                Entered::Suspension => preserved.extend(
                    after[position]
                        .iter()
                        .filter(|&&value| !data.defines(value))
                        .map(|&value| (value, func.repr_of(value))),
                ),
                Entered::Arrival => {
                    // Live AFTER, plus this instruction's operands, minus what it
                    // defines: an operand is needed because the instruction runs on
                    // arrival, and a result is not because it does not exist yet.
                    preserved.extend(
                        after[position]
                            .iter()
                            .copied()
                            .chain(data.inst.operands())
                            .filter(|value| !data.defines(*value))
                            .map(|value| (value, func.repr_of(value))),
                    );
                }
            }
        }
    }

    // Program order, so that a label's number is its position. Blocks are
    // visited in creation order and instructions within a block in program
    // order, so this only matters when a later block was created earlier.
    points.sort_by_key(|held| held.at);
    for (index, held) in points.iter_mut().enumerate() {
        held.label = ResumeLabel(index as u32);
    }

    SuspendPlan {
        points,
        spill: SpillLayout::build(preserved),
    }
}
