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
use crate::guard::PointId;

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
    fn prim(
        &mut self,
        into: &mut FuncBuilder,
        prim: Prim,
        args: &[MachineValue],
    ) -> Result<MachineValue, String>;

    /// A call to a named entry point of the runtime.
    fn entry(
        &mut self,
        into: &mut FuncBuilder,
        entry: EntryId,
        args: &[MachineValue],
    ) -> Result<MachineValue, String>;
}

/// Why a function could not be lowered.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Unlowerable {
    /// A guard or a fall, which needs the side exit of `deopt-lateral.md` D3.
    NeedsSideExit(PointId),
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

    for block in func.block_ids() {
        into.switch_to(blocks[&block]);
        for inst in &func.block(block).insts {
            let held = func.inst(*inst);
            let lowered = match &held.op {
                Op::Const(Const::Declared(index)) => {
                    ops.declared(into, *index).map_err(Unlowerable::Language)?
                }
                Op::Const(value) => constant(into, value),
                Op::Prim { prim, args } => {
                    let of_args = read(args, &values)?;
                    ops.prim(into, *prim, &of_args)
                        .map_err(Unlowerable::Language)?
                }
                Op::Call { callee, args } => match callee {
                    Callee::Entry(entry) => {
                        let of_args = read(args, &values)?;
                        ops.entry(into, *entry, &of_args)
                            .map_err(Unlowerable::Language)?
                    }
                    Callee::Func(_) | Callee::Dynamic(_) => {
                        return Err(Unlowerable::NeedsCallee);
                    }
                },
                Op::Guard { point, .. } => return Err(Unlowerable::NeedsSideExit(*point)),
            };
            values.insert(held.result, lowered);
        }

        let Some(end) = &func.block(block).terminator else {
            return Err(Unlowerable::Machine(format!(
                "block {} has no terminator; `verify` was not run",
                block.0
            )));
        };
        match end {
            Terminator::Jump { target, args } => {
                let of_args = read(args, &values)?;
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
                let else_args = read(else_args, &values)?;
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
        Const::Int(held) => ConstDecl::Scalar {
            repr: Repr::I32,
            bits: ScalarBits(*held as i32 as u32 as u64),
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
