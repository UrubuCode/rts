//! What the language supplies, and what it is told when a function cannot be lowered.
//!
//! Apart from `mod.rs` because that file passed this crate's ceiling of 500 lines, and
//! this is the seam it already had: the QUESTIONS a language answers, and the walk that
//! asks them.

use rts_cranelift::ir::{FuncBuilder, ValueId as MachineValue};
use rts_cranelift::repr::Repr;

use crate::cfg::{EntryId, Prim, ValueId};
use crate::guard::{Assertion, PointId};

/// What the language has to supply for its own operations.
///
/// One method per thing this module is forbidden to know. A front end implements
/// it beside its [`crate::domain::Domain`], and the two answer about the same
/// tables.
pub trait MachineOps {
    /// The representation a block parameter holds.
    ///
    /// Asked of the language because the answer comes from what a pass proved
    /// about the value, and what was proved is a fact in the language's own
    /// lattice. Answering the generic representation is always sound and always
    /// gives up whatever was proved.
    fn param_repr(&mut self, value: ValueId) -> Repr;

    /// A constant the language declared, by its own index.
    fn declared(&mut self, into: &mut FuncBuilder, index: u32) -> Result<MachineValue, String>;

    /// What a primitive computes.
    ///
    /// # Why the instruction travels beside the machine values
    ///
    /// Because without it a language cannot use what it proved, and this trait was
    /// short by exactly that for as long as nothing implemented it. `param_repr` above
    /// hands over a [`ValueId`] and says the answer *"comes from what a pass proved
    /// about the value"* -- and then this method received machine values only, so a
    /// front end could declare a parameter as an integer and still have no way to
    /// lower `+` as a machine add, because it could not ask what its OPERANDS were
    /// proved to be.
    ///
    /// That is the whole point of the type domain arriving at the boundary and being
    /// unusable there. A language holds its own inferred types -- `infer` answers them
    /// per [`ValueId`] -- so identity is all that was missing.
    ///
    /// # Why the whole instruction and not the operand ids
    ///
    /// One parameter instead of three, and the other two are worth having: `result` is
    /// what the answer's representation is a fact about, and `at` is the source
    /// position a fault record needs. A signature that handed over the ids alone would
    /// be back here the first time a lowering wanted to report where it failed.
    fn prim(
        &mut self,
        into: &mut FuncBuilder,
        prim: Prim,
        args: &[MachineValue],
        inst: &crate::cfg::Inst,
    ) -> Result<MachineValue, String>;

    /// What an assertion narrows to, in the machine's own vocabulary.
    ///
    /// `None` is "this assertion narrows to something no representation names", which
    /// is honest for a language whose `is a string` means a reference to a heap value
    /// the machine would have to be told the layout of. The guard is then refused
    /// rather than approximated.
    fn asserted_repr(&mut self, assertion: Assertion) -> Option<Repr>;

    /// This value in the representation a block parameter declares.
    ///
    /// # Why an edge needs this at all
    ///
    /// Because the two sides of a jump get their representation from different places. A
    /// block parameter's comes from [`Self::param_repr`] -- what a pass PROVED about the
    /// value -- and an argument's comes from whatever instruction produced it. Those
    /// agree most of the time and not always: a join of a guarded double with an integer
    /// literal is proved `Double`, while the literal arrives as an integer.
    ///
    /// The machine refuses the mismatch by design, and is right to: changing a
    /// representation is never implicit there. So the LANGUAGE says how to get from one
    /// to the other, because which conversions are sound is a fact about its lattice and
    /// not about the graph.
    ///
    /// Found by measurement rather than by reading: 133 of the roughly 350 refusals over
    /// `bench/` were this one error, and every test passed with it in place.
    fn coerce(
        &mut self,
        into: &mut FuncBuilder,
        value: MachineValue,
        want: Repr,
    ) -> Result<MachineValue, String>;

    /// The side exit: what happens when a guard fails.
    ///
    /// # Why the LANGUAGE answers this and the machine does not
    ///
    /// Because where a fall LANDS is an arrangement between two bodies of one
    /// function, and which two bodies those are is not something this crate can know.
    /// `deopt-lateral.md` states the arrangement -- the tier landed in is the generic
    /// body of the same function -- and naming that body is the front end's or the
    /// host's, never a graph's.
    ///
    /// It must TERMINATE the block it is given, and the block is one nothing else
    /// reaches: a guard's failure edge, created for this and entered on no other path.
    ///
    /// `live` is every value that existed where the guard stood, in order. For a guard
    /// at the entry that is exactly the parameters, which is the case this exists for;
    /// `lower` refuses any other, so an implementation never has to reconstruct state
    /// it was not handed.
    fn fall(
        &mut self,
        into: &mut FuncBuilder,
        point: PointId,
        live: &[MachineValue],
    ) -> Result<(), String>;

    /// A call through a value, with a receiver where the language passes one.
    ///
    /// # Why this is one question and not two
    ///
    /// Because a call with a receiver and a call without one differ in what they HAND
    /// OVER and not in what they are, and every language that has methods has both. A pair
    /// of methods would make the boundary answer twice about one operation, which is how
    /// the two come to disagree about the arity or the order.
    ///
    /// `None` for the receiver is a plain call. The language decides what either becomes:
    /// this crate does not know whether a receiver is an argument, a register or a frame
    /// slot, and `rts_cranelift::abi::Convention` does not either -- it is about linkage
    /// and tail calls and reserves nothing for one.
    fn call_value(
        &mut self,
        into: &mut FuncBuilder,
        callee: MachineValue,
        receiver: Option<MachineValue>,
        args: &[MachineValue],
        inst: &crate::cfg::Inst,
    ) -> Result<MachineValue, String>;

    /// A call to a named entry point of the runtime.
    ///
    /// Takes the instruction for the same reason [`Self::prim`] does, although an
    /// entry's own signature is fixed: what its ANSWER is proved to be is still the
    /// language's fact, and a boundary where one of two call shapes can consult the
    /// lattice is a boundary that will be asked why.
    fn entry(
        &mut self,
        into: &mut FuncBuilder,
        entry: EntryId,
        args: &[MachineValue],
        inst: &crate::cfg::Inst,
    ) -> Result<MachineValue, String>;

    /// What a raise throws and a handler catches, for this language.
    ///
    /// `rts_cranelift::unwind::Handler` matches on a `Tag`, and what may be thrown is the
    /// one question `unwind`'s own header refuses to answer for a language -- one
    /// language catches everything with one clause, another matches a type per clause.
    /// So it is asked here, and `None` -- the default -- is a language that has not said,
    /// whose regions and raises are refused as [`Unlowerable::NeedsHandlerTag`].
    ///
    /// One tag and not one per region: a language that matched per clause would need
    /// the clause's, and would widen this question when it arrives rather than before.
    fn exception_tag(&mut self) -> Option<rts_cranelift::unwind::Tag> {
        None
    }

    /// Whether the block about to be lowered is part of a CLEANUP.
    ///
    /// A cleanup is a piece the machine copies onto every path out of its region, and
    /// the machine's verifier refuses a piece that leaves by anything but finishing --
    /// a throw inside one is a second exit the unwinding knows nothing about. A language
    /// that follows a raising operation with a check that re-raises needs to know it is
    /// inside one, and which blocks those are is structure: everything reachable from a
    /// region's cleanup before it finishes. Told before each block; the default ignores
    /// it, which is right for a language that emits no such check.
    fn in_cleanup(&mut self, inside: bool) {
        let _ = inside;
    }

    /// What handing a value out at a suspension is, for this language, or `None` where it
    /// has not said -- the default, refused as [`Unlowerable::NeedsFrameTransform`].
    ///
    /// The SUSPENSION is neutral and this crate emits it: the machine's own instruction,
    /// whose frame `rts_cranelift::frame::resumable_form` rewrites. What the parked frame
    /// hands out, and to whom, is not: one language stores it where a resumer reads it,
    /// another returns it through a channel. So that half is asked, before the suspension.
    /// `value` is `None` where the program handed out nothing.
    ///
    /// The function being lowered must be declared as one that may suspend; the machine's
    /// verifier refuses the instruction in any other, which is the check this relies on.
    fn hand_out(
        &mut self,
        into: &mut FuncBuilder,
        value: Option<MachineValue>,
    ) -> Option<Result<(), String>> {
        let _ = (into, value);
        None
    }

    /// What a `return` hands back, in the representation the signature declared.
    ///
    /// Asked of the language because the SIGNATURE is the language's: whether every
    /// function returns one generic word -- what a caller that cannot know the callee
    /// has to be able to receive -- or what a pass proved, is a calling convention this
    /// crate does not choose. The default hands the value back as it is, which is right
    /// for a signature declared from the proof.
    fn returned(
        &mut self,
        into: &mut FuncBuilder,
        value: MachineValue,
    ) -> Result<MachineValue, String> {
        let _ = into;
        Ok(value)
    }
}

/// Why a function could not be lowered.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Unlowerable {
    /// A suspension, which needs the frame transform of `rts_cranelift::frame`.
    ///
    /// # Why the transform is the machine's and not expressible here
    ///
    /// Because it is decided by LIVENESS and spends STACK. Turning a function into a
    /// resumable one means finding what is live across each suspension, choosing a
    /// record to hold it, and choosing a resume position to re-enter at — three
    /// answers rule 3 of this crate's README puts on the machine's side, and
    /// `frame::resumable_form` already holds all three.
    ///
    /// So the suspension is emitted here as the machine's instruction, and the transform
    /// is applied by whoever places the function. What is still refused under this name
    /// is a language that has not said what handing a value out IS --
    /// [`MachineOps::hand_out`] answering `None`.
    NeedsFrameTransform,
    /// A guard or a fall, which needs the side exit of `deopt-lateral.md` D3.
    NeedsSideExit(PointId),
    /// A protected region, which needs the language to say what a handler catches.
    ///
    /// The region itself is neutral and this crate holds it. What a handler CATCHES is
    /// not: one language catches everything with one clause, another matches on a
    /// type, a third has a tag per raise site -- and `rts_cranelift::unwind::Handler`
    /// carries a `Tag` for exactly that reason. So the tag arrives through
    /// `MachineOps`, and until it does this refuses by name rather than inventing one.
    NeedsHandlerTag(crate::region::RegionId),
    /// A receiver, which needs the machine to decide how one reaches a callee.
    ///
    /// Named apart from [`Self::NeedsCallee`] because it is a different missing
    /// thing: a callee needs a registry and a signature, a receiver needs a
    /// CONVENTION. Counting the two together would hide which of them a corpus is
    /// actually waiting on.
    NeedsReceiverConvention,
    /// A call to a function of this program, named by its number.
    ///
    /// It needs the machine's id for that function -- a registry and a signature a
    /// caller holds and this signature does not take yet. A call to a VALUE used to
    /// be refused under this name too, and needed neither: it is
    /// [`MachineOps::call_value`] with no receiver.
    NeedsCallee,
    /// The machine refused what was built, with what it said.
    ///
    /// Carried as its text because a machine build error is the machine's type
    /// and this enum travels to the language, which must not have to match on it.
    Machine(String),
    /// The language's own lowering refused, with its reason.
    Language(String),
}
