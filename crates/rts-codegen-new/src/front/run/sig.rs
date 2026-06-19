//! Function ABI signatures for the whole-program lowering.
//!
//! Increment 4 compiles a *module* of user functions plus a synthesized
//! `__rtsn_main` for the top-level code, and cross-function calls must agree on
//! the ABI at each boundary. The rule (design pilar 2): a parameter / return is
//! carried UNBOXED in its native register when the front-end proves it
//! monomorphic-numeric (via the repr lattice [`crate::front::repr_map::repr_of`]),
//! otherwise it is a [`crate::value::PolyValue`] (`Tagged`, an `i64` raw word).
//!
//! [`FnSig`] freezes that decision per function so both the callee's prologue
//! and every call site box/unbox each value to match.

use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_module::Module;

use rts_hir::{HirExpr, HirFunc, HirStmt};

use crate::front::repr_map::repr_of;
use crate::repr::Repr;

use super::lower::cl_type;

/// The chosen ABI representation of every parameter and the return of one user
/// function. A `None` return repr means the function returns no value (`void` —
/// used for `__rtsn_main`).
#[derive(Clone, Debug)]
pub struct FnSig {
    pub name: String,
    pub params: Vec<Repr>,
    pub ret: Option<Repr>,
    /// True for an `async`/generator function. Such a function cannot be a sound
    /// first-class VALUE this increment (its call returns a Promise / it suspends);
    /// reifying it BAILS. Direct calls in the numeric subset are unaffected.
    pub is_async: bool,
    /// Index INTO [`Self::params`] of a trailing REST parameter (`...items`), if
    /// the function declares one. The index spans the FULL param list (including a
    /// leading `this` for methods/constructors). A rest param contributes exactly
    /// ONE `Repr` (always `Tagged` — the slot holds the packed rest array). `None`
    /// when the function has a fixed arity. Call sites consult this to pack a call's
    /// trailing args into a fresh array (F3b).
    pub rest_param: Option<usize>,
    /// Per-param FILLABILITY, parallel to [`Self::params`] (FULL index, including a
    /// leading `this` for methods/constructors). `fillable[i]` is `true` iff that
    /// param is OMITTABLE — optional (`x?`) OR has a default (`y = expr`). A
    /// fillable trailing arg may be left out at the call site (`marshal_call_args`
    /// pushes `undefined`); `this` and the rest param are never fillable. A fillable
    /// param is always carried `Tagged` (it can hold `undefined` / a default of any
    /// type), set in [`Self::of_func`].
    pub fillable: Vec<bool>,
    /// True when the function carries a synthesized leading `this` parameter
    /// (`params[0]`) that is NOT a class receiver — a FREE function (top-level
    /// `function F(){…}` or a `function` expression) whose body references `this`.
    /// A class method/constructor binds `this` via `this_class` and is NOT marked
    /// here. A PLAIN call `F(args)` to a `has_this` function prepends `undefined`
    /// as the receiver (Phase 1; `new F()` passing a real instance is Phase 2).
    pub has_this: bool,
    /// The user-CLASS this function provably RETURNS, when its body's `return`
    /// statements all construct the SAME known class (`return new Matcher(..)`).
    /// Lets a chained method call on the call RESULT (`expect(x).toBe(y)` — the
    /// `rts:test` matcher pattern) dispatch statically on that class. `None` when
    /// the return class is not provable. Filled by `compile_program` (which has the
    /// class table); `of_func`/`main_sig` leave it `None`.
    pub ret_class: Option<String>,
    /// True when the declared return type is an ARRAY (`T[]`). Lets a call/method
    /// RESULT be treated as a proven array (`for (const v of m.values())`,
    /// `o.keys().length`, `const ks = o.keys()`). Derived from `HirType::Array`
    /// (preserved through `parse_type_annotation`, unlike a class name).
    pub ret_array: bool,
}

impl FnSig {
    /// Derive the ABI signature of a user function from its HIR types: each
    /// numeric annotation rides its native register; anything else is `Tagged`.
    /// The return follows the same rule (a non-numeric / `Unknown` return — which
    /// is what an inferred-from-`console.log` body produces — becomes `Tagged`).
    pub fn of_func(func: &HirFunc) -> FnSig {
        // A param that is OPTIONAL or has a DEFAULT is omittable → fillable, and is
        // carried `Tagged` (it can hold `undefined` or a default of any type — the
        // `??`/undefined-fill machinery needs the boxed word). The `this`/rest params
        // are never marked fillable here (the rest param is detected below, and a
        // `this` synthesized param carries no `optional`/`has_default`).
        let fillable: Vec<bool> = func
            .params
            .iter()
            .map(|p| p.optional || p.has_default)
            .collect();
        // Params that are CALLED as a function in the body (`fn(...)`). A function
        // VALUE is a `TAG_FUNCTION` PolyValue word; if such a param were carried in a
        // native numeric repr (its declared `i64`/`number` — the `rts:test` bundle
        // types its callbacks `fn: i64`, the old engine's function-handle
        // convention), the call-site coercion would UNBOX the boxed word as an
        // integer and corrupt it, so the later `fn()` jumps to a garbage address
        // (stack overflow / SIGILL). Force every called param to `Tagged` so the
        // function word rides verbatim and the indirect invoke gets the real value.
        let called = params_called_as_fn(func);
        let params: Vec<Repr> = func
            .params
            .iter()
            .map(|p| {
                if p.optional || p.has_default || called.contains(&p.name) {
                    Repr::Tagged
                } else {
                    repr_for_param(&p.ty)
                }
            })
            .collect();
        // A trailing REST parameter (`...items`). TS requires the rest param to be
        // LAST; a valid program never has a non-last variadic. We detect ONLY the
        // last-variadic shape (ignore any earlier `variadic` flag, which valid TS
        // cannot produce) and record its index. The rest param's own repr is kept
        // as computed above (an annotated `...xs: number[]` is `HirType::Array` →
        // `Tagged`), which is the array word it carries.
        let rest_param = match func.params.last() {
            Some(p) if p.variadic => Some(func.params.len() - 1),
            _ => None,
        };
        // `has_this`: the function carries a synthesized leading `this` param
        // (`params[0].name == "this"`). This is true for class methods/constructors
        // AND for a FREE function whose body referenced `this` (the Phase 1 transform
        // prepends the same param). `of_func` cannot tell the two apart by the param
        // alone, so it sets the flag for BOTH; `compile_program` then CLEARS it for
        // class fns (those in `fn_this_class`), because the flag's consumer — the
        // plain direct-call `F(args)` undefined-receiver prepend — is a free-function
        // behavior (a class super-ctor call passes `this` explicitly in its args).
        let has_this = func.params.first().is_some_and(|p| p.name == "this");
        debug_assert!(
            func.params.iter().rev().skip(1).all(|p| !p.variadic),
            "non-last variadic parameter in `{}` (invalid TS)",
            func.name
        );
        // A `void`-returning function (e.g. a synthesized class constructor)
        // returns NO value: its ABI return is `None`, and the lowerer emits the
        // trailing `return` on fall-through (a value-returning fn that falls
        // through is ill-formed and bails).
        if matches!(func.ret, rts_hir::HirType::Void) {
            return FnSig {
                name: func.name.clone(),
                params,
                ret: None,
                is_async: func.is_async,
                rest_param,
                fillable,
                has_this,
                ret_class: None,
                ret_array: false,
            };
        }
        // The declared return repr — trusted in general (an explicit `boolean` /
        // `i64` annotation, or a `function`-decl's body-inferred type, is correct).
        let declared = repr_or_tagged(repr_of(&func.ret));
        // ONE narrow correction: the parser assigns the `i64` DEFAULT to an
        // expression-bodied arrow (incl. the hoisted `const f = (x: number) => …`
        // form) even when the body returns a `number` (Float64). Detect EXACTLY
        // that — declared `Int64` but every body `return e` is provably Float64 —
        // and use Float64. This fixes the arrow-default bug WITHOUT touching
        // functions whose returns are cross-fn calls / unknown (where the body
        // type is unreliable and the declared annotation must win, e.g. a mutually
        // recursive `boolean` predicate returning the peer's call).
        let mut ret = if declared == Repr::Int64 && all_returns_are_float64(func) {
            Repr::Float64
        } else {
            declared
        };
        // SOUNDNESS GUARD against the disguised-double carrier. The HIR types an
        // arithmetic body (`a - b`, `a * b`, …) as `number` (→ `Float64`) REGARDLESS
        // of operand reprs, but when a parameter is `Tagged` (untyped) the body
        // actually LOWERS to a `Tagged` PolyValue word (the generic `__rtsadp_*`
        // path), not a native `f64`. Typing such a function as `Float64`/`Int*`
        // -returning would force an UNSOUND carrier: the return coercion
        // `Tagged → Float64` `bitcast`s a tagged-int/string word (a value already in
        // the NaN-box space) through `f64`. That round-trip used to survive only by
        // a paired no-op `bitcast` at the call site — but `emit_box_double`'s NaN
        // canonicalization (needed so a genuine computed `NaN` does not read back as
        // a boxed tag — `Math.sqrt(-4)`, `0/0`) would clobber the disguised word to
        // `NaN`. Keeping a Tagged-param function's return `Tagged` (its honest body
        // repr) removes the carrier entirely, so canonicalization only ever sees
        // genuine doubles. A numeric-param function (the legitimate arrow-default
        // case `(x: number) => x*2`) is unaffected — its body really is native f64.
        if ret != Repr::Tagged
            && func
                .params
                .iter()
                .any(|p| repr_for_param(&p.ty) == Repr::Tagged)
        {
            ret = Repr::Tagged;
        }
        // A desugared eager GENERATOR returns the `__gen_buf` ARRAY word (a `Tagged`
        // PolyValue), but the parser stamps its declared return type `i64` (→ would
        // force `Int64`). Coercing the array word to `Int64` corrupts it (`g()` then
        // yields garbage). Keep the return `Tagged` — its honest body repr.
        let is_eager_generator = body_is_eager_generator(func);
        if is_eager_generator {
            ret = Repr::Tagged;
        }
        FnSig {
            name: func.name.clone(),
            params,
            ret: Some(ret),
            is_async: func.is_async,
            rest_param,
            fillable,
            has_this,
            ret_class: None,
            // An array declared return (`T[]`) OR a desugared eager GENERATOR (its
            // body ends `return __RTS_GEN_FINISH(__gen_buf, ret)`, handing back the
            // buffer ARRAY) — both make the call result a proven iterable array.
            ret_array: matches!(func.ret, rts_hir::HirType::Array(_)) || is_eager_generator,
        }
    }

    /// The synthesized top-level `__rtsn_main`: no params, no return.
    pub fn main_sig() -> FnSig {
        FnSig {
            name: "__rtsn_main".to_string(),
            params: Vec::new(),
            ret: None,
            is_async: false,
            rest_param: None,
            fillable: Vec::new(),
            has_this: false,
            ret_class: None,
            ret_array: false,
        }
    }

    /// Build the Cranelift `Signature` for this function under the host call conv.
    pub fn to_cranelift(&self, module: &dyn Module) -> Signature {
        let mut sig = Signature::new(module.isa().default_call_conv());
        for &p in &self.params {
            sig.params.push(AbiParam::new(cl_type(p)));
        }
        if let Some(r) = self.ret {
            sig.returns.push(AbiParam::new(cl_type(r)));
        }
        sig
    }
}

/// The ABI repr of a parameter: a proven numeric annotation stays unboxed; an
/// unannotated (`Unknown`) or non-numeric parameter is `Tagged`.
fn repr_for_param(ty: &rts_hir::HirType) -> Repr {
    repr_or_tagged(repr_of(ty))
}

/// Whether the function has at least one `return e` and EVERY such `return e` is
/// provably a `Float64` (`number`/`f64`) expression. This is the precise
/// signature of the parser's expression-bodied-arrow `i64`-default bug: the
/// declared ret is `i64` but the body actually returns a `number`. We only
/// override the declared ret in this exact case (see [`FnSig::of_func`]); a
/// `return` whose type is `Unknown` (a cross-fn call) makes this `false`, so the
/// declared annotation is kept (correct for recursive predicates).
fn all_returns_are_float64(func: &HirFunc) -> bool {
    let mut any = false;
    let mut all_float = true;
    walk_returns(&func.body, &mut any, &mut all_float);
    any && all_float
}

/// Walk the lowering-subset statements; set `any` if any `return e` is seen and
/// clear `all_float` unless every `return e` is a provable Float64 expression.
fn walk_returns(stmts: &[HirStmt], any: &mut bool, all_float: &mut bool) {
    for s in stmts {
        match s {
            HirStmt::Return(Some(e)) => {
                *any = true;
                if repr_of(&e.ty) != Repr::Float64 {
                    *all_float = false;
                }
            }
            HirStmt::If { then, else_, .. } => {
                walk_returns(then, any, all_float);
                if let Some(e) = else_ {
                    walk_returns(e, any, all_float);
                }
            }
            HirStmt::While { body, .. } | HirStmt::Block(body) => {
                walk_returns(body, any, all_float);
            }
            _ => {}
        }
    }
}

/// A native repr passes through; everything else collapses to `Tagged` (no
/// `Bool` is exposed across the call boundary as anything other than its i64
/// carrier, which `cl_type` already handles — `Tagged` vs `Bool` only differ in
/// the value layer's interpretation, both are i64 registers).
fn repr_or_tagged(r: Repr) -> Repr {
    if r.is_unboxed() { r } else { Repr::Tagged }
}

/// The set of PARAMETER names that appear as a CALL CALLEE (`name(...)`) anywhere
/// in `func`'s body — i.e. params used as first-class functions. Such a param must
/// be carried `Tagged` (a `TAG_FUNCTION` PolyValue word), never a native numeric
/// repr, so the indirect invoke gets the real function value (see `of_func`).
/// Whether `func` is a desugared EAGER generator: its body contains a call to the
/// parser sentinel `__RTS_GEN_FINISH` (which a `function*` desugars to — it returns
/// the `__gen_buf` array). Such a function's result is a proven iterable array.
fn body_is_eager_generator(func: &HirFunc) -> bool {
    use rts_hir::ir::HirExprKind;
    fn expr_calls(e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Call { callee, args } => {
                matches!(&callee.kind, HirExprKind::Ident(n) if n == "__RTS_GEN_FINISH")
                    || expr_calls(callee)
                    || args.iter().any(expr_calls)
            }
            HirExprKind::Bin { lhs, rhs, .. }
            | HirExprKind::Assign { target: lhs, value: rhs }
            | HirExprKind::AssignOp { target: lhs, value: rhs, .. } => {
                expr_calls(lhs) || expr_calls(rhs)
            }
            HirExprKind::MethodCall { object, args, .. } => {
                expr_calls(object) || args.iter().any(expr_calls)
            }
            HirExprKind::Unary { operand, .. } | HirExprKind::Cast { expr: operand, .. } => {
                expr_calls(operand)
            }
            _ => false,
        }
    }
    fn stmt_has(s: &HirStmt) -> bool {
        match s {
            HirStmt::Return(Some(e)) | HirStmt::Expr(e) | HirStmt::Throw(e) => expr_calls(e),
            HirStmt::Let { init: Some(e), .. } | HirStmt::Const { init: e, .. } => expr_calls(e),
            HirStmt::If { then, else_, .. } => {
                then.iter().any(stmt_has)
                    || else_.as_ref().is_some_and(|e| e.iter().any(stmt_has))
            }
            HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. }
            | HirStmt::ForOf { body, .. }
            | HirStmt::ForIn { body, .. }
            | HirStmt::Block(body) => body.iter().any(stmt_has),
            _ => false,
        }
    }
    func.body.iter().any(stmt_has)
}

fn params_called_as_fn(func: &HirFunc) -> std::collections::HashSet<String> {
    let param_names: std::collections::HashSet<&str> =
        func.params.iter().map(|p| p.name.as_str()).collect();
    let mut called: std::collections::HashSet<String> = std::collections::HashSet::new();

    fn walk_expr(
        e: &rts_hir::HirExpr,
        params: &std::collections::HashSet<&str>,
        out: &mut std::collections::HashSet<String>,
    ) {
        use rts_hir::ir::HirExprKind as K;
        if let K::Call { callee, args } = &e.kind {
            if let K::Ident(name) = &callee.kind {
                if params.contains(name.as_str()) {
                    out.insert(name.clone());
                }
            }
            walk_expr(callee, params, out);
            for a in args {
                walk_expr(a, params, out);
            }
            return;
        }
        // Recurse into every child expression of the other forms.
        match &e.kind {
            K::Bin { lhs, rhs, .. } | K::AssignOp { target: lhs, value: rhs, .. } | K::Assign { target: lhs, value: rhs } => {
                walk_expr(lhs, params, out);
                walk_expr(rhs, params, out);
            }
            K::Unary { operand, .. }
            | K::Cast { expr: operand, .. }
            | K::Await(operand)
            | K::Spread(operand)
            | K::PreInc(operand)
            | K::PreDec(operand)
            | K::PostInc(operand)
            | K::PostDec(operand) => walk_expr(operand, params, out),
            K::MethodCall { object, args, .. } => {
                walk_expr(object, params, out);
                for a in args {
                    walk_expr(a, params, out);
                }
            }
            K::New { args, .. } => {
                for a in args {
                    walk_expr(a, params, out);
                }
            }
            K::Member { object, .. } => walk_expr(object, params, out),
            K::Index { object, index } => {
                walk_expr(object, params, out);
                walk_expr(index, params, out);
            }
            K::Ternary { cond, then, else_ } => {
                walk_expr(cond, params, out);
                walk_expr(then, params, out);
                walk_expr(else_, params, out);
            }
            K::Array(items) => {
                for it in items {
                    walk_expr(it, params, out);
                }
            }
            K::Object(fields) => {
                for (_, v) in fields {
                    walk_expr(v, params, out);
                }
            }
            K::Seq(items) => {
                for it in items {
                    walk_expr(it, params, out);
                }
            }
            _ => {}
        }
    }

    fn walk_stmts(
        stmts: &[HirStmt],
        params: &std::collections::HashSet<&str>,
        out: &mut std::collections::HashSet<String>,
    ) {
        for s in stmts {
            match s {
                HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::Throw(e) => {
                    walk_expr(e, params, out)
                }
                HirStmt::Let { init: Some(e), .. } | HirStmt::Const { init: e, .. } => {
                    walk_expr(e, params, out)
                }
                HirStmt::If { cond, then, else_ } => {
                    walk_expr(cond, params, out);
                    walk_stmts(then, params, out);
                    if let Some(e) = else_ {
                        walk_stmts(e, params, out);
                    }
                }
                HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
                    walk_expr(cond, params, out);
                    walk_stmts(body, params, out);
                }
                HirStmt::For { init, cond, update, body } => {
                    if let Some(i) = init {
                        walk_stmts(std::slice::from_ref(i), params, out);
                    }
                    if let Some(c) = cond {
                        walk_expr(c, params, out);
                    }
                    if let Some(u) = update {
                        walk_expr(u, params, out);
                    }
                    walk_stmts(body, params, out);
                }
                HirStmt::ForOf { iterable, body, .. } => {
                    walk_expr(iterable, params, out);
                    walk_stmts(body, params, out);
                }
                HirStmt::ForIn { object, body, .. } => {
                    walk_expr(object, params, out);
                    walk_stmts(body, params, out);
                }
                HirStmt::Block(body) => walk_stmts(body, params, out),
                HirStmt::Try { body, catch, finally } => {
                    walk_stmts(body, params, out);
                    if let Some(c) = catch {
                        walk_stmts(&c.body, params, out);
                    }
                    if let Some(f) = finally {
                        walk_stmts(f, params, out);
                    }
                }
                HirStmt::Switch { discriminant, cases } => {
                    walk_expr(discriminant, params, out);
                    for c in cases {
                        if let Some(t) = &c.test {
                            walk_expr(t, params, out);
                        }
                        walk_stmts(&c.body, params, out);
                    }
                }
                _ => {}
            }
        }
    }

    walk_stmts(&func.body, &param_names, &mut called);
    called
}
