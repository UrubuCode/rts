//! An operator over operands nothing proved, taking the instruction when they turn out to
//! be numbers -- the shape `emit/expr.rs::emit_guarded` gives the running engine's.
//!
//! ```text
//!   guard a is a double ── not one ──┐
//!          │                         │
//!   guard b is a double ── not one ──┤
//!          │                         │
//!     instruction                  slow: the runtime's call
//!          │                         │
//!          └──────► join(value) ◄────┘
//! ```
//!
//! # Why the boundary does this, and not the graph's second tier
//!
//! Because nothing has to be RECONSTRUCTED when the guess is wrong. A guard of the MIR's
//! own falls to the generic twin and hands it the live state, which is what makes it a
//! speculation; here the failure edge is the very call the generic arm would have made,
//! at the same point, over the same operands. So it is a way of LOWERING one operation,
//! the choice `generic.rs` makes for every other row, and the graph cannot tell the two
//! apart. The day the specialised tier runs, a graph that proved the operand already
//! reaches the instruction ahead of this and never asks.
//!
//! # Why it was needed at all
//!
//! Without it, a function this stage took answered `5 + i` with a call where the running
//! engine answers it with a guard and an add -- a disabled optimisation, which passes every
//! correctness test there is. `rts-host/tests/literal_guard_gate.rs` is what said so.
//!
//! # A literal that can never be a double asks nothing
//!
//! The lattice knows a string, a boolean, `null` or an object when the program wrote one,
//! and such an operand cannot take the instruction at any value. Guarding its partner
//! would only add a branch in front of the same call, so the row goes straight to it --
//! which is the running engine's rule too, pinned by the same test.

use rts_cranelift::ir::{CmpOp, FuncBuilder, NumOp, ValueId as MachineValue};
use rts_cranelift::repr::Repr;
use rts_mir::cfg::ValueId;

use super::{JsMachine, machine};
use crate::domain::{JsPrim, Type};

impl JsMachine<'_> {
    /// The instruction for `which` over two operands already in the double domain, or
    /// `None` where the row has none.
    pub(super) fn numeric_instruction(
        &mut self,
        into: &mut FuncBuilder,
        which: JsPrim,
        left: MachineValue,
        right: MachineValue,
    ) -> Result<Option<MachineValue>, String> {
        // EVERY ARITHMETIC ROW ANSWERS A DOUBLE in this lattice, so every one of these is
        // a float instruction and there is no integer arm to choose.
        let compare = match which {
            JsPrim::LessThan => Some(CmpOp::Lt),
            JsPrim::GreaterThan => Some(CmpOp::Gt),
            JsPrim::LessOrEqual => Some(CmpOp::Le),
            JsPrim::GreaterOrEqual => Some(CmpOp::Ge),
            // `a === b` over two numbers coerces nothing and can call nothing, and
            // `a == b` over two numbers IS `a === b` -- the loose algorithm's first step,
            // for operands of one type.
            JsPrim::StrictEquals | JsPrim::LooseEquals => Some(CmpOp::Eq),
            _ => None,
        };
        if let Some(op) = compare {
            return into.compare(op, left, right).map(Some).map_err(machine);
        }
        if let Some(bits) = self.bits(into, which, left, right)? {
            return Ok(Some(bits));
        }
        let op = match which {
            JsPrim::Add => NumOp::Add,
            JsPrim::Subtract => NumOp::Sub,
            JsPrim::Multiply => NumOp::Mul,
            JsPrim::Divide => NumOp::Div,
            // THE REMAINDER OF TWO DOUBLES IS `fmod`, which the machine has an exact
            // sequence for only where the divisor is a power of two -- it answers that
            // itself, from the constant, and refuses every other divisor. What is left
            // is `NumberRemainder`, the unboxed call the running engine makes for the
            // same case: `emit/expr.rs::proven_instruction` is the same two steps.
            JsPrim::Remainder => {
                if let Ok(exact) = into.arith(NumOp::Rem, left, right) {
                    return Ok(Some(exact));
                }
                return self
                    .call_runtime(
                        into,
                        crate::runtime::RuntimeOp::NumberRemainder,
                        &[left, right],
                    )
                    .map(Some);
            }
            // `**` OF TWO DOUBLES is the unboxed call the running engine makes for it
            // (`emit/expr.rs`, `Proven::NumberCall`): there is no instruction to try.
            JsPrim::Exponent => {
                return self
                    .call_runtime(
                        into,
                        crate::runtime::RuntimeOp::NumberExponent,
                        &[left, right],
                    )
                    .map(Some);
            }
            _ => return Ok(None),
        };
        into.arith(op, left, right).map(Some).map_err(machine)
    }

    /// The bitwise rows over two doubles, as `emit/expr.rs` emits them: `ToInt32` both,
    /// the instruction, and the count of a shift masked to five bits -- the language's
    /// rule, emitted rather than left to a backend's modulo. The answer stays an `I32`,
    /// which is the representation the lattice's `Int32` names; `>>>` alone answers
    /// the unsigned conversion back to a double.
    fn bits(
        &mut self,
        into: &mut FuncBuilder,
        which: JsPrim,
        left: MachineValue,
        right: MachineValue,
    ) -> Result<Option<MachineValue>, String> {
        use rts_cranelift::ir::BitOp;
        let (op, shifts) = match which {
            JsPrim::BitAnd => (BitOp::And, false),
            JsPrim::BitOr => (BitOp::Or, false),
            JsPrim::BitXor => (BitOp::Xor, false),
            JsPrim::ShiftLeft => (BitOp::Shl, true),
            JsPrim::ShiftRight => (BitOp::Shr, true),
            JsPrim::ShiftRightUnsigned => (BitOp::ShrUnsigned, true),
            _ => return Ok(None),
        };
        let left = into.to_int32(left).map_err(machine)?;
        let mut right = into.to_int32(right).map_err(machine)?;
        if shifts {
            let mask = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
                repr: Repr::I32,
                bits: rts_cranelift::ir::ScalarBits(31),
            });
            let mask = into.use_const(mask);
            right = into.bitwise(BitOp::And, right, mask).map_err(machine)?;
        }
        let bits = into.bitwise(op, left, right).map_err(machine)?;
        match which {
            JsPrim::ShiftRightUnsigned => into.to_f64_unsigned(bits).map(Some).map_err(machine),
            _ => Ok(Some(bits)),
        }
    }

    /// `which` over two operands of which at least one is unproved, guarded into the
    /// instruction and falling to the runtime's call -- or `None` where the row has no
    /// instruction or an operand can never be a double.
    pub(super) fn guarded(
        &mut self,
        into: &mut FuncBuilder,
        which: JsPrim,
        of: &[ValueId],
        args: &[MachineValue],
    ) -> Result<Option<MachineValue>, String> {
        let ([a, b], [held_a, held_b]) = (args, of) else {
            return Ok(None);
        };
        let proven = |held: &ValueId| matches!(self.types.of(*held), Type::Int32 | Type::Double);
        let (a_proven, b_proven) = (proven(held_a), proven(held_b));
        // AN OPERAND THAT MAY BE A DOUBLE is one the lattice knows nothing about. Anything
        // it DID name -- a string, a boolean, a singleton, an object -- is never one.
        let may_be = |held: &ValueId, is_proven: bool| {
            is_proven || matches!(self.types.of(*held), Type::Anything)
        };
        if !may_be(held_a, a_proven) || !may_be(held_b, b_proven) {
            return Ok(None);
        }
        let rows = [
            JsPrim::Add,
            JsPrim::Subtract,
            JsPrim::Multiply,
            JsPrim::Divide,
            JsPrim::Remainder,
            JsPrim::Exponent,
            JsPrim::BitAnd,
            JsPrim::BitOr,
            JsPrim::BitXor,
            JsPrim::ShiftLeft,
            JsPrim::ShiftRight,
            JsPrim::ShiftRightUnsigned,
            JsPrim::LessThan,
            JsPrim::GreaterThan,
            JsPrim::LessOrEqual,
            JsPrim::GreaterOrEqual,
            JsPrim::StrictEquals,
            JsPrim::LooseEquals,
        ];
        if !rows.contains(&which) {
            return Ok(None);
        }

        let slow = into.create_block();
        // Each operand is narrowed only if something is still to be established about it,
        // and ONE value guarded twice is one guard: `x + x` hands the same value to both
        // sides, and inside the first guard's success it cannot fail the second.
        let mut narrow = |into: &mut FuncBuilder, held: MachineValue| {
            let narrowed = into.create_block();
            let param = into.add_block_param(narrowed, Repr::F64);
            into.guard(held, Repr::F64, (narrowed, &[]), (slow, &[]))
                .map_err(machine)?;
            into.switch_to(narrowed);
            Ok::<_, String>(param)
        };
        let left = match a_proven {
            true => self.as_double(into, *a)?,
            false => narrow(into, *a)?,
        };
        let right = if b_proven {
            self.as_double(into, *b)?
        } else if a == b {
            left
        } else {
            narrow(into, *b)?
        };
        let fast = self
            .numeric_instruction(into, which, left, right)?
            .ok_or_else(|| format!("{which:?} is a guarded row with no instruction"))?;
        let fast_block = into.current();

        // THE SLOW EDGE IS THE GENERIC ARM, over the operands as they arrived -- so what it
        // answers is what this row answered before the guard existed.
        into.switch_to(slow);
        let answered = self
            .generic(into, which, &[*a, *b])?
            .ok_or_else(|| format!("{which:?} is a guarded row with no generic form"))?;
        let joined = into.repr_of(answered);
        let join = into.create_block();
        let result = into.add_block_param(join, joined);
        into.jump(join, &[answered]).map_err(machine)?;

        into.switch_to(fast_block);
        let fast = match into.repr_of(fast) == joined {
            true => fast,
            false => super::coerced(into, fast, joined)?,
        };
        into.jump(join, &[fast]).map_err(machine)?;
        into.switch_to(join);
        Ok(Some(result))
    }
}
