//! `JSON.stringify(x)` and `JSON.parse(s)`, called by what they are.
//!
//! # What this removes, and what it does not
//!
//! The ordinary call reads the global `JSON`, reads `stringify` off it through
//! the chain cache, and enters the generic call machinery with a receiver and
//! an argument vector — to reach a function the compiler could have named.
//! Measured 2026-09-19 on `target/release/rts.exe`: `JSON.stringify(42)` cost
//! 238 ns against 139 for `String(42)`, which writes the same two digits into
//! the same kind of cell.
//!
//! It does NOT make serialisation an instruction sequence, and the reason is
//! the membership rule for an entry point rather than effort: the walk reads
//! the heap, the answer is allocated, and a double's shortest digits are not
//! something a machine vocabulary should grow. What a call site can know that
//! the runtime cannot is which FUNCTION this is — so that is what is decided
//! here, and the walk stays where the heap is.
//!
//! # The proof
//!
//! `Ctx::json_primordial` — `primordial::only_a_base`, over the whole program:
//! nothing writes `JSON` or a member of it, nothing reaches `eval` or
//! `globalThis`, and the name never appears except as the base of a member
//! expression, so no copy of the object exists for a write to go through.
//! Shadowing is the scope's question, asked here exactly as `machine_operation`
//! asks it of `Math`.
//!
//! # Why one argument only
//!
//! A replacer, an indentation and a reviver are all the ordinary call, which is
//! right for each. So is a spread, whose length is not known here; and so is NO
//! argument, where the ordinary call's `undefined` is already the answer.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::scope::Scope;
use super::{Ctx, EmitResult};
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, ExprKind, Spreadable};

/// The call through its entry point, or `None` for the ordinary call.
pub(super) fn emit(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<Option<ValueId>> {
    if !ctx.json_primordial {
        return Ok(None);
    }
    let ExprKind::Member {
        object,
        property,
        optional: false,
    } = &callee.kind
    else {
        return Ok(None);
    };
    let ExprKind::Ident(name) = &object.kind else {
        return Ok(None);
    };
    if ctx.names.text(*name) != "JSON" || scope.lookup(*name).is_some() {
        return Ok(None);
    }
    let op = match ctx.names.text(*property) {
        "stringify" => RuntimeOp::JsonStringify,
        "parse" => RuntimeOp::JsonParse,
        _ => return Ok(None),
    };
    let [Spreadable::Single(only)] = arguments else {
        return Ok(None);
    };
    let argument = super::expr::emit_expr(builder, scope, ctx, only)?;
    let argument = super::expr::tagged(builder, argument);
    Ok(Some(super::expr::call(builder, ctx, op, &[argument])?[0]))
}
