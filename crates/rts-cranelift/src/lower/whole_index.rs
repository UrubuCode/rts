//! Which double values are PROVEN whole numbers, so an element access can skip
//! the whole-number half of its position test.
//!
//! [`super::run_read`] and [`super::run_write`] convert the position to a 64-bit
//! integer with saturation, convert it back and compare, to reject a fraction, an
//! infinity and a NaN. For a loop's counter that round trip is pure overhead: the
//! counter is a whole number by construction. The bounds half of the test (the
//! unsigned compare against the length) is untouched and always stays; a negative
//! or enormous whole number still fails it.
//!
//! # The proof
//!
//! A value is whole when it is
//!
//! * a conversion of a 32-bit (or 64-bit) integer to a double, or a double
//!   constant that is a whole number of magnitude at most 2^31;
//! * the sum or difference of a whole value and a BOUNDED whole addend (one of the
//!   two forms above, magnitude at most 2^32); or
//! * a block parameter of a block that is not the entry, whose every incoming
//!   edge passes a whole value.
//!
//! The parameter rule is an induction over execution: the first value a
//! parameter takes comes from an edge whose argument is whole by the other rules,
//! and every later one is whole by the same rules applied to values that already
//! were. It is solved as a greatest fixed point, so a counter that is fed by its
//! own increment is whole exactly when its entry values are.
//!
//! # Why a sum stays a whole number and never an infinity
//!
//! A whole double plus a whole double is a whole double or an infinity; above
//! 2^53 every double is whole. The addend is bounded by 2^32, so reaching the
//! largest finite double from a start below 2^63 takes more than 2^990 additions:
//! no program executes that, and the doubling case (`x = x + x`) is excluded
//! because the addend must be a bounded form rather than another whole value.
//!
//! # What is declined
//!
//! A function with a protected region or a suspension: a handler or a resumption
//! point is an entry the edges below do not list. Everything the proof does not
//! name keeps the full test.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BlockCall, ConstDecl, Function, Inst, NumOp, Terminator, ValueId,
};
use crate::repr::Repr;

/// The largest magnitude of a constant addend (2^31).
const SMALL: f64 = 2_147_483_648.0;

/// Every transfer a terminator makes, with its arguments.
fn calls(t: &Terminator) -> Vec<&BlockCall> {
    match t {
        Terminator::Jump(call) => vec![call],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![then_block, else_block],
        Terminator::Guard { ok, fail, .. } | Terminator::GuardType { ok, fail, .. } => {
            vec![ok, fail]
        }
        Terminator::CachedGet { hit, miss, .. }
        | Terminator::CachedGetIndirect { hit, miss, .. }
        | Terminator::CachedGetKeyed { hit, miss, .. }
        | Terminator::RunRead { hit, miss, .. }
        | Terminator::RunWrite { hit, miss, .. }
        | Terminator::CellAlloc { hit, miss }
        | Terminator::CachedSet { hit, miss, .. } => vec![hit, miss],
        Terminator::Return(_)
        | Terminator::Throw { .. }
        | Terminator::TailCall { .. }
        | Terminator::TailCallIndirect { .. }
        | Terminator::CleanupDone
        | Terminator::Trap(_) => Vec::new(),
    }
}

/// How a double-valued instruction result is proven.
enum Rule {
    /// Whole, and bounded by 2^32.
    Bounded,
    /// Whole when `a` is whole and `b` is bounded (or, for a sum, the reverse).
    Sum(ValueId, ValueId, bool),
}

/// The values of `func` proven to be whole numbers.
pub(super) fn plan(func: &Function) -> HashSet<ValueId> {
    if !func.regions.is_empty() || func.signature.may_suspend {
        return HashSet::new();
    }

    let mut rules: HashMap<ValueId, Rule> = HashMap::new();
    let mut params: HashMap<ValueId, (usize, usize)> = HashMap::new();
    let mut incoming: HashMap<usize, Vec<&BlockCall>> = HashMap::new();

    for (id, data) in func.blocks() {
        for (slot, param) in data.params.iter().enumerate() {
            if func.repr_of(*param) == Repr::F64 && id != func.entry {
                params.insert(*param, (id.index(), slot));
            }
        }
        for inst in &data.insts {
            let Some(d) = func.inst(*inst) else { continue };
            let Some(result) = d.result() else { continue };
            if func.repr_of(result) != Repr::F64 {
                continue;
            }
            match &d.inst {
                Inst::ToF64(_) | Inst::ToF64Unsigned(_) => {
                    rules.insert(result, Rule::Bounded);
                }
                Inst::Const(c) => {
                    if let Some(ConstDecl::Scalar {
                        repr: Repr::F64,
                        bits,
                    }) = func.constant(*c)
                    {
                        let x = f64::from_bits(bits.0);
                        if x.is_finite() && x.fract() == 0.0 && x.abs() <= SMALL {
                            rules.insert(result, Rule::Bounded);
                        }
                    }
                }
                Inst::FloatArith(op @ (NumOp::Add | NumOp::Sub), a, b) => {
                    rules.insert(result, Rule::Sum(*a, *b, *op == NumOp::Add));
                }
                _ => {}
            }
        }
        if let Some(t) = &data.terminator {
            for call in calls(t) {
                incoming.entry(call.block.index()).or_default().push(call);
            }
        }
    }

    // Bounded values never leave the set; a sum or a parameter starts in and is
    // taken out when its reason fails.
    let mut whole: HashSet<ValueId> = rules.keys().copied().collect();
    whole.extend(params.keys().copied());
    let bounded: HashSet<ValueId> = rules
        .iter()
        .filter(|(_, r)| matches!(r, Rule::Bounded))
        .map(|(v, _)| *v)
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        let mut drop: Vec<ValueId> = Vec::new();
        for (v, rule) in &rules {
            if let Rule::Sum(a, b, commutes) = rule {
                let ok = (whole.contains(a) && bounded.contains(b))
                    || (*commutes && whole.contains(b) && bounded.contains(a));
                if !ok && whole.contains(v) {
                    drop.push(*v);
                }
            }
        }
        for (v, (block, slot)) in &params {
            if !whole.contains(v) {
                continue;
            }
            let ok = incoming.get(block).is_some_and(|edges| {
                !edges.is_empty()
                    && edges.iter().all(|call| {
                        call.args
                            .get(*slot)
                            .is_some_and(|arg| whole.contains(arg))
                    })
            });
            if !ok {
                drop.push(*v);
            }
        }
        for v in drop {
            if whole.remove(&v) {
                changed = true;
            }
        }
    }

    whole
}

/// Whether a position may be used: whole AND inside. A position proven whole
/// needs only the bounds compare it was given.
pub(super) fn position_usable(
    builder: &mut cranelift_frontend::FunctionBuilder,
    known: bool,
    whole: cranelift_codegen::ir::Value,
    position: cranelift_codegen::ir::Value,
    inside: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    use cranelift_codegen::ir::condcodes::FloatCC;
    use cranelift_codegen::ir::{InstBuilder, types};
    if known {
        return inside;
    }
    let again = builder.ins().fcvt_from_sint(types::F64, whole);
    let exact = builder.ins().fcmp(FloatCC::Equal, again, position);
    builder.ins().band(exact, inside)
}
