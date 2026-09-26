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

use crate::cfg::{BlockId, Callee, Const, Func, Op, Terminator, ValueId};

pub use ops::{MachineOps, Unlowerable};

mod ops;
mod regions;

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
    // EVERY OTHER BLOCK IS CREATED HERE, and in the REGION TREE's order rather than the
    // block list's: the machine places a block in a region when the block is made, from
    // the regions open at that moment, so a block has to be made with its region open.
    // `regions.rs` walks the tree once, opening and closing each region around the
    // blocks it holds. The regions are closed again before a single instruction is
    // emitted, and the builder is told to let a block made while emitting inherit the
    // region of the block being emitted -- which is what every continuation is.
    crate::lower::regions::create_blocks(func, into, ops, &mut blocks)?;
    into.inherit_block_regions();
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

    let cleanups = regions::cleanup_blocks(func);
    for block in func.block_ids() {
        into.switch_to(blocks[&block]);
        ops.in_cleanup(cleanups.contains(&block));
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
                    //
                    // AND A CALL WITH NO RECEIVER IS THE SAME QUESTION, asked with one
                    // thing fewer. It was refused beside a call to a function of the
                    // program, under one name, while `call_value` already took its
                    // receiver as optional -- so the most common call there is, `f(x)`
                    // over a value, stopped at a registry it never needed.
                    (Callee::Dynamic(value), held_receiver) => {
                        let callee = one(*value, &values)?;
                        let receiver = match held_receiver {
                            Some(held) => Some(one(*held, &values)?),
                            None => None,
                        };
                        let of_args = read(args, &values)?;
                        ops.call_value(into, callee, receiver, &of_args, held)
                            .map_err(Unlowerable::Language)?
                    }
                    (_, Some(_)) => return Err(Unlowerable::NeedsReceiverConvention),
                    (Callee::Entry(entry), None) => {
                        let of_args = read(args, &values)?;
                        ops.entry(into, *entry, &of_args, held)
                            .map_err(Unlowerable::Language)?
                    }
                    (Callee::Func(_), None) => {
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
                Op::Suspend { value } => {
                    let handed = value.map(|held| one(held, &values)).transpose()?;
                    match ops.hand_out(into, handed) {
                        None => return Err(Unlowerable::NeedsFrameTransform),
                        Some(said) => said.map_err(Unlowerable::Language)?,
                    }
                    into.suspend()
                }
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
            // A CLEANUP IS COPIED, and copying it is the machine's business: which paths
            // need it is `unwind::plan_unwind` and `plan_normal_exit`, computed from the
            // region tree. So the end of one is only a statement that it ended.
            Terminator::CleanupDone => into.cleanup_done(),
            // A RAISE IS A THROW OF THE LANGUAGE'S TAG, and where it lands is the region
            // its block is in -- which is why there is no destination to name.
            Terminator::Raise(value) => {
                let Some(tag) = ops.exception_tag() else {
                    return Err(Unlowerable::NeedsHandlerTag(
                        func.region_of(block).unwrap_or(crate::region::RegionId(0)),
                    ));
                };
                let value = one(*value, &values)?;
                into.throw(tag, value);
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
                    let held = ops.returned(into, held).map_err(Unlowerable::Language)?;
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
