//! The LOWERING half of Tier-0 escape analysis: replace the construction with a
//! list of `Variable` definitions, and the field read with a `use_var`.
//!
//! Both hooks are no-ops unless [`super::super::clifflags::escape_analysis`] is on
//! and the local survived [`super::scan`]. Nothing here re-decides eligibility —
//! the analysis is upstream, on purpose, so a hook cannot accidentally widen it.

use std::collections::HashMap;

use cranelift_codegen::ir::Value;
use cranelift_frontend::Variable;
use cranelift_module::Module;

use rts_hir::HirExpr;

use crate::front::error::FrontResult;
use crate::repr::Repr;

use super::super::lower::{Local, Lowerer, Val, cl_type};

/// A scalar-replaced object: the fields it WOULD have had, each living in its own
/// Cranelift `Variable`. There is no heap word anywhere — the object does not
/// exist at run time.
#[derive(Clone)]
pub(crate) struct ScalarObj {
    /// The class it would have been an instance of. Kept for diagnostics and for
    /// the defensive refusal in the member-write path.
    pub class: String,
    /// `(field name, variable, repr)` in constructor order. Linear search: a
    /// scalar-replaced class has a handful of fields, and a `HashMap` per object
    /// would cost more to build than the scans it saves.
    pub fields: Vec<(String, Variable, Repr)>,
}

impl ScalarObj {
    fn field(&self, prop: &str) -> Option<(Variable, Repr)> {
        self.fields
            .iter()
            .find(|(n, _, _)| n == prop)
            .map(|(_, v, r)| (*v, *r))
    }
}

impl Lowerer<'_, '_, '_> {
    /// `let/const <name> = new <class>(args)` where the pre-scan proved `<name>`
    /// does not escape: emit the constructor's field stores straight into fresh
    /// `Variable`s and record the object. Returns `false` when the site is not
    /// eligible, in which case the caller lowers the ordinary heap `new`.
    ///
    /// ## What this emits
    ///
    /// ```text
    ///   const p = new Point(a, b + 1)
    ///
    ///   t0 = coerce(lower(a),     ctor param 0's Repr)     ← arguments first,
    ///   t1 = coerce(lower(b + 1), ctor param 1's Repr)       left to right
    ///   v_x = lower(<ctor's `this.x =` initializer>[params := t0, t1])
    ///   v_y = lower(<ctor's `this.y =` initializer>[params := t0, t1])
    /// ```
    ///
    /// ## Why the side-effect order is identical
    ///
    /// The heap path evaluates the arguments left to right, allocates, then runs
    /// the constructor body top to bottom. This path evaluates the arguments left
    /// to right into temps, then evaluates the field initializers top to bottom in
    /// the recipe's (i.e. the constructor's) order. The only step removed is the
    /// allocation, which is not observable for an object that never escapes. The
    /// arguments are coerced through the CONSTRUCTOR'S OWN parameter `Repr`s (from
    /// its `FnSig`), so a `x: number` parameter widens or narrows exactly as the
    /// real call would have — the substitution is faithful, not approximate.
    ///
    /// A constructor that does ANYTHING beyond field stores never gets a recipe
    /// (see [`super::recipe`]), and passing the object as `this` to a call is a
    /// bail in [`super::scan`] — so the "constructor with real behaviour" case
    /// cannot arrive here.
    pub(in crate::front::run) fn try_lower_scalar_new(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        class: &str,
        args: &[HirExpr],
    ) -> FrontResult<bool> {
        if !super::super::clifflags::escape_analysis() {
            return Ok(false);
        }
        if self.scalar_candidates.get(name).map(String::as_str) != Some(class) {
            return Ok(false);
        }
        // A top-level `let` in `__rts_startup` is diverted to a GCELL — a
        // program-global, readable from every other function, which is escape by
        // definition. Same for a local the closure machinery already promoted to a
        // runtime cell. Both are checked here rather than in the scan because both
        // are properties of the LOWERING CONTEXT, not of the HIR.
        if (self.is_main && self.block_depth <= 1)
            || self.cell_locals.contains(name)
            || self.gcells.contains_key(name)
        {
            return Ok(false);
        }
        let Some(desc) = self.classes.get(class).cloned() else {
            return Ok(false);
        };
        let Some(recipe) = desc.scalar_ctor.clone() else {
            return Ok(false);
        };
        let Some(sig) = self.sigs.get(&desc.ctor).cloned() else {
            return Ok(false);
        };
        // `sig.params[0]` is the implicit `this`; user parameter `i` is `i + 1`.
        if sig.params.len() != recipe.params.len() + 1 || args.len() != recipe.params.len() {
            return Ok(false);
        }

        // ---- 1. arguments → temps, left to right, in the ctor's param Repr ----
        let seq = self.escape_seq;
        self.escape_seq += 1;
        let mut subst: HashMap<String, String> = HashMap::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            let val = self.lower_expr(module, arg)?;
            let repr = sig.params[i + 1];
            let w = self.coerce(val, repr)?;
            let var = self.builder.declare_var(cl_type(repr));
            self.builder.def_var(var, w);
            // A generated name, so it can never collide with a user local: the
            // `__rts_ea` prefix plus a per-construction sequence number.
            let temp = format!("__rts_ea{seq}_{}", recipe.params[i]);
            self.locals.insert(temp.clone(), Local { var, repr });
            subst.insert(recipe.params[i].clone(), temp);
        }

        // ---- 2. field initializers → one Variable each, in ctor order ----
        let mut fields: Vec<(String, Variable, Repr)> = Vec::with_capacity(recipe.fields.len());
        for (field, init) in &recipe.fields {
            let substituted = super::recipe::subst_params(init, &subst);
            let val = self.lower_expr(module, &substituted)?;
            // "Typed by the field's Repr" — and the field's Repr IS its
            // initializer's, exactly because the scan refuses any later write to
            // it. If writes are ever allowed, this is the line that needs a join.
            let var = self.builder.declare_var(cl_type(val.repr));
            self.builder.def_var(var, val.v);
            fields.push((field.clone(), var, val.repr));
        }

        self.scalar_objs.insert(
            name.to_string(),
            ScalarObj {
                class: class.to_string(),
                fields,
            },
        );
        Ok(true)
    }

    /// `<local>.<field>` where `<local>` was scalar-replaced: a `use_var`, no load,
    /// no IC site, no shape compare. Returns `None` when the receiver is not a
    /// scalar-replaced local, so the caller falls through to the ordinary member
    /// path.
    ///
    /// Sees through casts (`(p as any).x`) for the same reason the scan does — a
    /// cast is a type assertion, not a different value.
    pub(in crate::front::run) fn try_scalar_field_read(
        &mut self,
        object: &HirExpr,
        prop: &str,
    ) -> Option<Val> {
        if self.scalar_objs.is_empty() {
            return None;
        }
        let base = super::scan::strip_casts(object);
        let rts_hir::ir::HirExprKind::Ident(n) = &base.kind else {
            return None;
        };
        let (var, repr) = self.scalar_objs.get(n)?.field(prop)?;
        let v: Value = self.builder.use_var(var);
        Some(Val::new(v, repr))
    }

    /// Whether `object` names a scalar-replaced local. Used by the member-WRITE
    /// path to refuse honestly: [`super::scan`] guarantees no write reaches a
    /// scalar-replaced local, so arriving here means the scan and the lowering
    /// disagree — and the only safe response to that is a compile-time bail, never
    /// a write to a heap object that does not exist.
    pub(in crate::front::run) fn scalar_obj_class(&self, object: &HirExpr) -> Option<&str> {
        if self.scalar_objs.is_empty() {
            return None;
        }
        let base = super::scan::strip_casts(object);
        let rts_hir::ir::HirExprKind::Ident(n) = &base.kind else {
            return None;
        };
        self.scalar_objs.get(n).map(|o| o.class.as_str())
    }
}
