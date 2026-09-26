//! The two prototype links an `extends` makes, and the one value that makes
//! only one of them.
//!
//! # Why `extends null` is not a degenerate case to refuse
//!
//! `class C extends null {}` is legal, and it is the only spelling that gives a
//! class an instance chain ending at its own `prototype` — which is what a
//! program reaches for when it wants objects with no `toString`, no
//! `hasOwnProperty` and no `Object.prototype` behind them at all. The
//! specification states it directly: for a heritage of `null`, `protoParent` is
//! `null` and `constructorParent` stays `%Function.prototype%`.
//!
//! So the two links come apart. A constructor heritage makes both —
//! `C.prototype.__proto__ = P.prototype` and `C.__proto__ = P` — and a `null`
//! heritage makes only the first, with `null` as the value, because there is no
//! `P` to inherit statics from.
//!
//! # Why a branch and not a runtime entry point
//!
//! The question is which of two link shapes to emit, and the answer is known
//! only at run time because the heritage is an arbitrary expression. An entry
//! point taking the constructor, the prototype and the heritage would put the
//! whole of `ClassDefinitionEvaluation`'s linking step in the runtime — where
//! the rest of the class lowering is here, and where rule 2 of this crate's
//! README says a language decision does not belong. What the machine already
//! offers is exactly what is missing: `is_singleton` asks the question and
//! `branch` acts on it.
//!
//! # What is deliberately NOT done here
//!
//! A heritage that is neither `null` nor a constructor is a `TypeError` in the
//! language and is linked as though it were a constructor here. Raising it
//! needs a throw the emitter can spell, which is the same gap every other
//! compile-time-known refusal in this crate records — named rather than
//! silently folded into the `null` arm, because folding it would answer
//! `extends 5` with a null prototype chain, which is a plausible wrong answer
//! rather than a loud one.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::{Ctx, EmitResult, UNPROVEN, expr};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::values::Singleton;

/// Links a class to its heritage, both sides of it.
///
/// `prototype_name` is the caller's already-interned `"prototype"`, threaded
/// through rather than re-interned so the two readers of it in one class
/// emission cannot come to disagree about the key.
pub(super) fn link(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    constructor: ValueId,
    prototype: ValueId,
    parent: ValueId,
    prototype_name: Name,
) -> EmitResult<()> {
    // Nothing PROVEN is a singleton — a proved double, boolean or reference
    // cannot be `null` — so the test has one answer and the machine refuses to
    // ask it. The same guard `choice::branch_on_nullish` states, for the same
    // reason.
    if builder.repr_of(parent) != UNPROVEN {
        return from_constructor(builder, ctx, constructor, prototype, parent, prototype_name);
    }

    let null = ctx.model.singleton(Singleton::Null);
    let is_null = builder.is_singleton(parent, null)?;
    let bare = builder.create_block();
    let inherits = builder.create_block();
    let join = builder.create_block();
    builder.branch(is_null, (bare, &[]), (inherits, &[]))?;

    builder.switch_to(bare);
    // Only the instance chain, and it ends here. The constructor's own
    // `[[Prototype]]` stays what `ClosureNew` gave it, which IS
    // `%Function.prototype%` — so `Object.getPrototypeOf(C) === Function.prototype`
    // without this arm writing anything.
    let nothing = expr::singleton(builder, ctx, Singleton::Null);
    expr::call(builder, ctx, RuntimeOp::SetPrototype, &[prototype, nothing])?;
    builder.jump(join, &[])?;

    builder.switch_to(inherits);
    from_constructor(builder, ctx, constructor, prototype, parent, prototype_name)?;
    builder.jump(join, &[])?;

    builder.switch_to(join);
    Ok(())
}

/// The ordinary heritage: both links, from a constructor.
fn from_constructor(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    constructor: ValueId,
    prototype: ValueId,
    parent: ValueId,
    prototype_name: Name,
) -> EmitResult<()> {
    let parent_prototype = super::property::emit_read(builder, ctx, parent, prototype_name)?;
    expr::call(
        builder,
        ctx,
        RuntimeOp::SetPrototype,
        &[prototype, parent_prototype],
    )?;
    // The link an implementation forgets, and whose absence shows only when a
    // program calls an inherited STATIC method.
    expr::call(
        builder,
        ctx,
        RuntimeOp::SetPrototype,
        &[constructor, parent],
    )?;
    Ok(())
}
