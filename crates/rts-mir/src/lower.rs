//! MIR to machine IR: the one module here that may name the machine.
//!
//! README rule 3. Everything else in this crate manipulates this crate's own
//! representation; this is where a block becomes a machine block, a block
//! parameter becomes one with a representation, and a terminator becomes a jump.
//!
//! # What this module CANNOT do, and who does it instead
//!
//! It cannot lower a [`Prim`]. Rule 4 says a primitive is an opaque index, and
//! that rule has teeth precisely here: turning `Prim(0)` into an `iadd` requires
//! knowing that index 0 is this language's addition, which is knowledge this
//! crate does not have and must not acquire. The next language numbers its table
//! differently and its addition is a different operation over different types.
//!
//! So the language supplies [`MachineOps`], and the division is exact:
//!
//! | this module | the language |
//! |---|---|
//! | blocks, parameters, terminators, order | what a primitive computes |
//! | mapping MIR values to machine values | what a declared constant is |
//! | which of the two tiers is being built | what an entry point answers |
//!
//! That is the same "reuse is by naming, not by selection" that
//! `rts-host/src/entries.rs` enforces on the runtime boundary, one layer up.
//!
//! # Why a guard is refused here rather than approximated
//!
//! A guard's failure leaves through a side exit that spills the live frame and
//! jumps into the other tier's resume label. The machine capability that does
//! the spilling is `rts_cranelift::frame`, and the module that would call it —
//! `frame/exit.rs` — does not exist yet: it is D3 of
//! `docs/engine/deopt-lateral.md`. Lowering a guard to a plain branch into a trap
//! would compile and would be a different program, so it answers
//! [`Unlowerable::NeedsSideExit`] instead.

use std::collections::BTreeMap;

use rts_cranelift::ir::{BlockId as MachineBlock, FuncBuilder, ValueId as MachineValue};
use rts_cranelift::repr::Repr;

use crate::cfg::{BlockId, Callee, Const, EntryId, Func, Op, Prim, Terminator, ValueId};
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
    /// So this is the one place a graph that is entirely well formed is refused for
    /// something the MACHINE has not been asked for yet, rather than for something the
    /// language has not declared.
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
    /// A call to a function of this program, or to a value.
    ///
    /// Both need a function registry and a signature, which a caller holds and
    /// this signature does not take yet.
    NeedsCallee,
    /// The machine refused what was built, with what it said.
    ///
    /// Carried as its text because a machine build error is the machine's type
    /// and this enum travels to the language, which must not have to match on it.
    Machine(String),
    /// The language's own lowering refused, with its reason.
    Language(String),
}

/// Lowers one MIR function into a machine function already under construction.
///
/// The caller owns the machine `Function` and its signature, which is why this
/// takes a builder rather than making one: the entry block's parameters come from
/// the signature, so whoever declared the signature has already decided them.
/// This maps the MIR entry block's parameters onto those, in order.
pub fn lower(
    func: &Func,
    into: &mut FuncBuilder,
    ops: &mut impl MachineOps,
    entry_params: &[MachineValue],
) -> Result<(), Unlowerable> {
    let mut blocks: BTreeMap<BlockId, MachineBlock> = BTreeMap::new();
    let mut values: BTreeMap<ValueId, MachineValue> = BTreeMap::new();

    // The entry block is the machine's own, and its parameters were declared by
    // the signature. Every other block is created here, BEFORE anything is
    // emitted, because a jump may name a block that appears later in the list.
    blocks.insert(func.entry(), into.current());
    for (at, param) in func.block(func.entry()).params.iter().enumerate() {
        let Some(held) = entry_params.get(at) else {
            return Err(Unlowerable::Machine(format!(
                "the signature declares {} parameters and the graph's entry block declares {}",
                entry_params.len(),
                func.block(func.entry()).params.len()
            )));
        };
        values.insert(*param, *held);
    }
    for block in func.block_ids() {
        if block == func.entry() {
            continue;
        }
        blocks.insert(block, into.create_block());
    }
    // Then the parameters, which a jump's arguments have to match. Done in a
    // second pass for the same reason the blocks were: an argument may be a value
    // defined in a block that has not been walked.
    for block in func.block_ids() {
        if block == func.entry() {
            continue;
        }
        let machine = blocks[&block];
        for param in &func.block(block).params {
            let repr = ops.param_repr(*param);
            values.insert(*param, into.add_block_param(machine, repr));
        }
    }

    // A REGION IS REFUSED before anything is emitted, because protection is a property
    // of a block and emitting the blocks first would produce a function whose
    // instructions are right and whose exception edges are absent.
    if let Some(region) = func.regions.first() {
        let _ = region;
        return Err(Unlowerable::NeedsHandlerTag(crate::region::RegionId(0)));
    }

    for block in func.block_ids() {
        into.switch_to(blocks[&block]);
        // WHETHER ANYTHING OBSERVABLE HAS HAPPENED in this block yet. The condition a
        // side exit needs, made structural: see the `Op::Guard` arm.
        let mut nothing_observed = true;
        for inst in &func.block(block).insts {
            let held = func.inst(*inst);
            let lowered = match &held.op {
                Op::Const(Const::Declared(index)) => {
                    ops.declared(into, *index).map_err(Unlowerable::Language)?
                }
                Op::Const(value) => constant(into, value),
                Op::Prim { prim, args } => {
                    let of_args = read(args, &values)?;
                    ops.prim(into, *prim, &of_args, held)
                        .map_err(Unlowerable::Language)?
                }
                Op::Call {
                    callee,
                    receiver,
                    args,
                } => match (callee, receiver) {
                    // A RECEIVER TRAVELS AS ITSELF, to a language that says what to do
                    // with one. This arm used to refuse, on the reasoning that how a
                    // receiver reaches a callee is a calling convention and therefore the
                    // machine's -- and that reasoning was wrong in the same way the
                    // `ThisValue` note was: `rts_cranelift::abi::Convention` is about
                    // linkage and tail calls and reserves nothing for a receiver, so the
                    // machine has no answer to give.
                    //
                    // It is the LANGUAGE's, which is why this hands the receiver over
                    // beside the callee and the arguments rather than packing it anywhere:
                    // the boundary is told what it is and decides what a call with one
                    // becomes. Packing it here is what would have invented a convention.
                    (Callee::Dynamic(value), Some(held_receiver)) => {
                        let callee = one(*value, &values)?;
                        let receiver = one(*held_receiver, &values)?;
                        let of_args = read(args, &values)?;
                        ops.call_value(into, callee, Some(receiver), &of_args, held)
                            .map_err(Unlowerable::Language)?
                    }
                    (_, Some(_)) => return Err(Unlowerable::NeedsReceiverConvention),
                    (Callee::Entry(entry), None) => {
                        let of_args = read(args, &values)?;
                        ops.entry(into, *entry, &of_args, held)
                            .map_err(Unlowerable::Language)?
                    }
                    (Callee::Func(_) | Callee::Dynamic(_), None) => {
                        return Err(Unlowerable::NeedsCallee);
                    }
                },
                Op::Guard {
                    assertion,
                    on,
                    point,
                } => {
                    // THE CONDITION THAT MAKES A SIDE EXIT BUILDABLE, and it is about
                    // what has been OBSERVED rather than about what has been built.
                    //
                    // A guard in the entry block with only PURE instructions before it
                    // can fall by handing the original arguments to the other tier and
                    // letting it run from the top. The other tier re-executes those same
                    // pure instructions, which is sound precisely because pure means
                    // nothing can tell: same operands, same answers, no heap read, no
                    // heap written, nothing allocated, nothing called, nothing raised.
                    //
                    // This started as "nothing but guards before it", which is the same
                    // idea read too narrowly -- a guard is pure, and so is the arithmetic
                    // a numeric function does before its first comparison. That narrower
                    // form refused every function in `bench/` that computed anything at
                    // all before speculating.
                    //
                    // ALLOCATION IS EXCLUDED and it is the interesting exclusion: an
                    // allocation before the guard would be performed twice, and the first
                    // object becomes garbage rather than a wrong answer. Excluded anyway,
                    // because `PURE` is a line this crate can check exactly and "produces
                    // only garbage" is a judgement it would have to argue.
                    //
                    // A guard behind anything else needs the frame reconstructed -- the
                    // arrival `rts_cranelift::frame` now has, plus the transfer -- and is
                    // refused by the same name it always was.
                    if block != func.entry() || !nothing_observed {
                        return Err(Unlowerable::NeedsSideExit(*point));
                    }
                    let input = one(*on, &values)?;
                    let Some(repr) = ops.asserted_repr(*assertion) else {
                        return Err(Unlowerable::Language(format!(
                            "{assertion:?} narrows to nothing this machine has a representation for"
                        )));
                    };
                    // THE NARROWED VALUE IS THE OK BLOCK'S FIRST PARAMETER, which is how
                    // it comes to exist only where the test held -- the machine's own
                    // words for why its guard is shaped this way. So the parameter is
                    // declared before the guard is emitted, because the machine checks
                    // that it is there.
                    let ok = into.create_block();
                    let narrowed = into.add_block_param(ok, repr);
                    let fail = into.create_block();
                    into.guard(input, repr, (ok, &[]), (fail, &[]))
                        .map_err(|held| Unlowerable::Machine(format!("{held:?}")))?;

                    // THE SIDE EXIT, which the language builds because which two bodies
                    // a fall is between is not something a graph knows.
                    into.switch_to(fail);
                    ops.fall(into, *point, entry_params)
                        .map_err(Unlowerable::Language)?;

                    into.switch_to(ok);
                    values.insert(held.result, narrowed);
                    continue;
                }
                Op::Suspend { .. } => return Err(Unlowerable::NeedsFrameTransform),
            };
            // AN OBSERVABLE INSTRUCTION closes the window, and a pure one does not. Read
            // from the EFFECT rather than from the operation, so a language that grows a
            // pure primitive gets the window widened without this crate naming it.
            if !held.effect.is_pure() {
                nothing_observed = false;
            }
            values.insert(held.result, lowered);
        }

        let Some(end) = &func.block(block).terminator else {
            return Err(Unlowerable::Machine(format!(
                "block {} has no terminator; `verify` was not run",
                block.0
            )));
        };
        match end {
            // THE SAME MISSING DECLARATION A PROTECTED REGION WAITS ON, and named as
            // that rather than as a second thing: which handlers a raise matches is
            // decided by its tag, and a tag is what may be thrown, which is the one
            // question `unwind`'s own header refuses to answer for a language.
            // A CLEANUP IS COPIED, and copying it is the machine's business: which
            // paths need it is `unwind::plan_unwind` and `plan_normal_exit`, computed
            // from the region tree. So this waits on the same declaration a region
            // does -- without a tag there is no plan to copy it into.
            Terminator::CleanupDone => {
                return Err(Unlowerable::NeedsHandlerTag(crate::region::RegionId(0)));
            }
            Terminator::Raise(_) => {
                return Err(Unlowerable::NeedsHandlerTag(crate::region::RegionId(0)));
            }
            Terminator::Jump { target, args } => {
                let of_args = read(args, &values)?;
                let of_args = matched(into, ops, func, *target, &blocks, &values, &of_args)?;
                into.jump(blocks[target], &of_args)
                    .map_err(|held| Unlowerable::Machine(format!("{held:?}")))?;
            }
            Terminator::Branch {
                condition,
                then_block,
                then_args,
                else_block,
                else_args,
            } => {
                let condition = one(*condition, &values)?;
                let then_args = read(then_args, &values)?;
                let then_args =
                    matched(into, ops, func, *then_block, &blocks, &values, &then_args)?;
                let else_args = read(else_args, &values)?;
                let else_args =
                    matched(into, ops, func, *else_block, &blocks, &values, &else_args)?;
                into.branch(
                    condition,
                    (blocks[then_block], &then_args),
                    (blocks[else_block], &else_args),
                )
                .map_err(|held| Unlowerable::Machine(format!("{held:?}")))?;
            }
            Terminator::Return(value) => match value {
                Some(held) => {
                    let held = one(*held, &values)?;
                    into.ret(&[held]);
                }
                None => into.ret(&[]),
            },
            Terminator::Fall(point) => return Err(Unlowerable::NeedsSideExit(*point)),
            // Nothing reaches here, and the machine's own way of saying so is a
            // trap. A block that falls off the end instead would be a function
            // with no terminator, which its verifier refuses.
            Terminator::Unreachable => {
                into.trap(rts_cranelift::ir::TrapCode::Unreachable);
            }
        }
    }
    Ok(())
}

/// A structural constant, which needs no language knowledge.
///
/// `Declared` is not here: it is the language's own table, and it is the one
/// constant shape this module hands back.
fn constant(into: &mut FuncBuilder, value: &Const) -> MachineValue {
    use rts_cranelift::ir::{ConstDecl, ScalarBits};
    let decl = match value {
        // THE REPRESENTATION COMES FROM THE VALUE, and the version that did not was a
        // silent wrong answer: `Const::Int` holds an `i64` and this declared every one of
        // them `I32`, truncating through `as i32`. `Const::Int(5_000_000_000)` became
        // 705_032_704 with nothing said.
        //
        // `I32` WHERE IT FITS and not `I64` always, because the narrow form is the one the
        // rest of the machine wants: `to_f64` accepts `I32` and refuses `I64`, so a
        // literal widened here would stop being usable in the double domain every
        // arithmetic row answers.
        Const::Int(held) => match i32::try_from(*held) {
            Ok(fits) => ConstDecl::Scalar {
                repr: Repr::I32,
                bits: ScalarBits(fits as u32 as u64),
            },
            Err(_) => ConstDecl::Scalar {
                repr: Repr::I64,
                bits: ScalarBits(*held as u64),
            },
        },
        Const::Float(held) => ConstDecl::Scalar {
            repr: Repr::F64,
            bits: ScalarBits(held.to_bits()),
        },
        Const::Bool(held) => return into.bool_constant(*held),
        Const::Declared(_) => unreachable!("a declared constant is the language's, not this one's"),
    };
    let id = into.declare_const(decl);
    into.use_const(id)
}

/// Each argument in the representation the target block's parameter declares.
///
/// Asked of the language per value rather than assumed, and only where the two differ:
/// a conversion emitted where none was needed is an instruction the program did not
/// write, which is the other way to get this wrong.
fn matched(
    into: &mut FuncBuilder,
    ops: &mut impl MachineOps,
    func: &Func,
    target: BlockId,
    blocks: &BTreeMap<BlockId, MachineBlock>,
    values: &BTreeMap<ValueId, MachineValue>,
    args: &[MachineValue],
) -> Result<Vec<MachineValue>, Unlowerable> {
    let _ = blocks;
    let declared = &func.block(target).params;
    let mut out = Vec::with_capacity(args.len());
    for (at, held) in args.iter().enumerate() {
        let Some(param) = declared.get(at) else {
            // ARITY IS THE MACHINE'S TO REFUSE, and it does, with the block it was about.
            // Reporting it here would be a second account of one fact.
            out.push(*held);
            continue;
        };
        let Some(machine_param) = values.get(param) else {
            out.push(*held);
            continue;
        };
        let want = into.repr_of(*machine_param);
        match into.repr_of(*held) == want {
            true => out.push(*held),
            false => out.push(
                ops.coerce(into, *held, want)
                    .map_err(Unlowerable::Language)?,
            ),
        }
    }
    Ok(out)
}

fn read(
    args: &[ValueId],
    values: &BTreeMap<ValueId, MachineValue>,
) -> Result<Vec<MachineValue>, Unlowerable> {
    args.iter().map(|held| one(*held, values)).collect()
}

fn one(
    value: ValueId,
    values: &BTreeMap<ValueId, MachineValue>,
) -> Result<MachineValue, Unlowerable> {
    values.get(&value).copied().ok_or_else(|| {
        Unlowerable::Machine(format!(
            "value {} is read before it is defined; `verify` was not run",
            value.0
        ))
    })
}
