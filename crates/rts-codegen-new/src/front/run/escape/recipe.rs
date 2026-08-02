//! The per-CLASS half of Tier-0 escape analysis: is this constructor REPLAYABLE
//! as a list of expressions, with no object in the middle?
//!
//! Scalar replacement is only sound if the construction's observable effects can
//! be reproduced without the object existing. That is a property of the CLASS, not
//! of a use site, so it is decided once — at class collection — and cached on the
//! [`super::super::class::ClassDesc`].
//!
//! ## The accepted shape, and nothing else
//!
//! A class qualifies iff ALL of:
//!
//! 1. **No superclass.** `extends` means a `super(args)` call inside the
//!    constructor — a call, with the half-built instance as `this`. That is an
//!    escape by the use-site rule too, and flattened parent fields would have to be
//!    replayed from a second constructor body this pass never sees.
//! 2. **No accessors** (`get x()` / `set x()`). A field read/write on an accessor
//!    property is a CALL in JS semantics; replacing it with a register read would
//!    delete an observable user function invocation.
//! 3. **The synthesized constructor's body is exactly a sequence of
//!    `this.<field> = <expr>` statements**, one per statement, nothing else. No
//!    `if`, no loop, no `return`, no local `let`, no bare call. A branch would make
//!    the field set depend on control flow, and the whole point of the flat
//!    Variable list is that it does not.
//! 4. **Each `<expr>` is PURE and STATICALLY NUMERIC** over the constructor's own
//!    parameters and numeric literals: literals, parameter identifiers, casts of
//!    those, and arithmetic/bitwise/unary operators over those. Explicitly NOT:
//!    a call (could throw, could capture `this`, could observe order), a `Member`
//!    or `Index` (a heap read whose value's JS kind we would then have to model),
//!    `this` itself (`this.b = this.a + 1` — a read of a slot mid-construction;
//!    replayable in principle, deliberately deferred), `new`, `await`, an arrow,
//!    a string/array/object literal.
//! 5. **Every field the class declares is assigned exactly once**, in order. A
//!    field left implicitly `undefined` would need a `Repr` this pass has no source
//!    for; a field assigned twice makes "the field's `Repr`" ambiguous.
//! 6. **A fixed, plain parameter list** — no rest, no default, no optional. Those
//!    make the argument→parameter mapping at the call site something other than
//!    positional, and `marshal_call_args` (which the replay path deliberately does
//!    not run) is where that logic lives.
//!
//! ### Why "statically numeric" rather than "any pure expression"
//!
//! Not soundness — semantics preservation of the *hints*. A field holding a
//! string, an array or an object instance is read elsewhere in the engine through
//! `field_strings` / `field_arrays` / `local_classes`, which turn `p.name.length`
//! or `p.items.push(x)` into native paths. A `Variable` carries a `Repr`, not those
//! facts, so scalar-replacing such a field would silently demote every downstream
//! access. Restricting to numeric fields means the value a read produces is
//! `Val::new(word, repr)` with `JsKind::Number` — bit-for-bit the same thing the
//! heap path would have produced, with no hint lost.
//!
//! Widening past this is the obvious next increment, and it needs the field's
//! heap-shape facts carried alongside the `Variable`, not a relaxation here.

use rts_hir::ir::{HirExpr, HirExprKind, HirFunc, HirLit, HirStmt, HirType, HirUnOp};

/// A constructor proven replayable as a flat expression list — the "recipe" a
/// construction site substitutes its arguments into.
///
/// Serde-derived because it rides [`super::super::class::ClassDesc`], which the
/// program/prelude caches round-trip.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ScalarCtor {
    /// The constructor's USER parameter names, in order (the implicit leading
    /// `this` excluded). A construction site binds argument `i` to a temp under a
    /// generated name and rewrites `params[i]` to it.
    pub params: Vec<String>,
    /// `(field name, initializer)` in CONSTRUCTOR ORDER — which is also the order
    /// the real constructor would have run the stores in, so replaying it keeps
    /// the side-effect order identical. The initializer names only [`Self::params`]
    /// and numeric literals (see the module doc).
    pub fields: Vec<(String, HirExpr)>,
}

/// Extract the recipe for class `class_name`, or `None` if the class does not fit
/// the accepted shape. `funcs` is the freshly synthesized function list for the
/// class (the constructor is the single `__rtsn_ctor_*` in it), matching how
/// `class::synth::collect_ctor_array_fields` finds it.
///
/// `fields` is the class's FLATTENED ordered field list, `has_parent` / `has_accessors`
/// the two structural disqualifiers from the module doc.
pub(in crate::front::run) fn extract_scalar_ctor(
    funcs: &[HirFunc],
    fields: &[String],
    has_parent: bool,
    has_accessors: bool,
) -> Option<ScalarCtor> {
    if has_parent || has_accessors || fields.is_empty() {
        return None;
    }
    let ctor = funcs.iter().find(|f| f.name.starts_with("__rtsn_ctor_"))?;

    // Params: drop the implicit leading `this`, refuse anything non-positional.
    let mut params: Vec<String> = Vec::new();
    for (i, p) in ctor.params.iter().enumerate() {
        if i == 0 && p.name == super::super::class::THIS {
            continue;
        }
        if p.variadic || p.has_default || p.optional {
            return None;
        }
        if !is_numeric_ty(&p.ty) {
            // A non-numeric parameter can still only reach a field through a
            // numeric-typed expression, which `expr_is_pure_numeric` would refuse
            // anyway — refusing here just makes the reason legible.
            return None;
        }
        params.push(p.name.clone());
    }

    // Body: exactly one `this.<field> = <pure numeric>` per statement.
    let mut out: Vec<(String, HirExpr)> = Vec::with_capacity(fields.len());
    for stmt in &ctor.body {
        let HirStmt::Expr(e) = stmt else {
            return None;
        };
        let HirExprKind::Assign { target, value } = &e.kind else {
            return None;
        };
        let HirExprKind::Member { object, prop } = &target.kind else {
            return None;
        };
        if !matches!(&object.kind, HirExprKind::Ident(n) if n == super::super::class::THIS) {
            return None;
        }
        if !expr_is_pure_numeric(value, &params) {
            return None;
        }
        // Exactly once, and only fields the class actually declares. A `this.z = 1`
        // for an undeclared `z` would be a shape transition on a real instance.
        if !fields.iter().any(|f| f == prop) || out.iter().any(|(f, _)| f == prop) {
            return None;
        }
        out.push((prop.clone(), (**value).clone()));
    }
    // Every declared field must be covered — an unassigned one is `undefined`, and
    // `undefined` has no numeric `Repr` for its Variable.
    if out.len() != fields.len() {
        return None;
    }
    Some(ScalarCtor { params, fields: out })
}

/// A HIR type the engine carries in a numeric register. `I128`/`U128` are excluded
/// on purpose — they are not a single Cranelift scalar on this path.
fn is_numeric_ty(t: &HirType) -> bool {
    matches!(
        t,
        HirType::Number
            | HirType::F32
            | HirType::F64
            | HirType::I8
            | HirType::I16
            | HirType::I32
            | HirType::I64
            | HirType::U8
            | HirType::U16
            | HirType::U32
            | HirType::U64
    )
}

/// The initializer is PURE (no call, no heap read, no allocation, cannot observe
/// or mutate anything) and STATICALLY NUMERIC. See the module doc for why both
/// halves are required.
///
/// `Cast` is seen THROUGH rather than walked past — the same lesson
/// `super::super::floatscan` was fixed for: `x as number` is a type ASSERTION
/// wrapping the operand, so whether the tree qualifies is decided entirely by what
/// is inside it. A scan that stops at a `Cast` is a scan that cannot see the value.
fn expr_is_pure_numeric(e: &HirExpr, params: &[String]) -> bool {
    match &e.kind {
        HirExprKind::Lit(HirLit::Int(_) | HirLit::Float(_) | HirLit::Number(_)) => true,
        // Only the constructor's OWN parameters. A free identifier would be a
        // module global or a capture, whose value at replay time is not this
        // expression's to assume.
        HirExprKind::Ident(n) => params.iter().any(|p| p == n),
        HirExprKind::Cast { expr, target } => is_numeric_ty(target) && expr_is_pure_numeric(expr, params),
        HirExprKind::Bin { op, lhs, rhs } => {
            (op.is_arithmetic() || op.is_bitwise())
                && expr_is_pure_numeric(lhs, params)
                && expr_is_pure_numeric(rhs, params)
        }
        HirExprKind::Unary { op, operand } => {
            matches!(op, HirUnOp::Neg | HirUnOp::Plus | HirUnOp::BitNot)
                && expr_is_pure_numeric(operand, params)
        }
        // Everything else — Call, MethodCall, New, Member, Index, Array, Object,
        // Arrow, Await, Ternary, Seq, Spread, the inc/dec forms, Raw — bails.
        // Ternary is *pure* but introduces control flow into the replay; deferred.
        _ => false,
    }
}

/// Rewrite every `Ident(p)` for `p` in `map` to `map[p]`. Only the node set
/// [`expr_is_pure_numeric`] admits is handled, because only that set can appear in
/// a [`ScalarCtor`] initializer — an unexpected node is returned unchanged, which
/// is safe precisely because it cannot occur.
pub(super) fn subst_params(e: &HirExpr, map: &std::collections::HashMap<String, String>) -> HirExpr {
    let kind = match &e.kind {
        HirExprKind::Ident(n) => match map.get(n) {
            Some(temp) => HirExprKind::Ident(temp.clone()),
            None => HirExprKind::Ident(n.clone()),
        },
        HirExprKind::Cast { expr, target } => HirExprKind::Cast {
            expr: Box::new(subst_params(expr, map)),
            target: target.clone(),
        },
        HirExprKind::Bin { op, lhs, rhs } => HirExprKind::Bin {
            op: *op,
            lhs: Box::new(subst_params(lhs, map)),
            rhs: Box::new(subst_params(rhs, map)),
        },
        HirExprKind::Unary { op, operand } => HirExprKind::Unary {
            op: *op,
            operand: Box::new(subst_params(operand, map)),
        },
        other => other.clone(),
    };
    HirExpr::new(kind, e.ty.clone())
}
