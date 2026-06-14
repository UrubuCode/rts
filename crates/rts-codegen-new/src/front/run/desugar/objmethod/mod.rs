//! P5.15 object-literal METHOD recovery.
//!
//! rts-hir's object-literal lowering (`lower_swc_expr`'s `Expr::Object` arm) keeps
//! only `KeyValue` / `Shorthand` props — it `filter_map`s away every `Method`,
//! `Getter`, `Setter`, computed, and spread prop. So a literal
//! `{ name: "x", greet() { return "hi " + this.name; } }` reaches the HIR as
//! `Object([("name", "x")])`, its `greet` gone.
//!
//! This pass re-reads the swc AST PAIRED with the HIR (the same per-unit positional
//! pairing the P5.8 template / P5.11 destructure recovery uses) and, for each object
//! literal that carries recoverable PLAIN methods, does two things:
//!
//! 1. Builds a content-keyed "literal class" ([`super::super::class::build_literal_class`])
//!    — a [`ClassDesc`](super::super::class) with the literal's fields + a synthesized
//!    `this`-first `HirFunc` per method — and registers it in the program's class
//!    table (two identical literals share ONE descriptor).
//! 2. PREPENDS a synthetic marker field `("__rtsl_class__", <class name>)` to the HIR
//!    `Object` node. [`super::super::obj`]'s `lower_object_literal` strips this marker
//!    (it never becomes a real slot) and records the local's class in `local_classes`,
//!    so `obj.method()` static-dispatches and `${obj}` / `obj + 1` / `String(obj)` run
//!    the `toString`/`valueOf` ToPrimitive chain — all through the EXISTING
//!    class-instance lowering, no new dispatch.
//!
//! ## Soundness — total bail on anything hard
//! A literal is recovered ONLY when EVERY non-field prop is a plain method
//! (non-generator, non-async, identifier name, simple-ident params). A getter,
//! setter, computed/generator/async method, spread, or a complex param makes the
//! literal keep NO class — every method call on it then BAILS (never a guess).
//! Object literals inside template interpolations are not yet real HIR at this stage
//! (still `Raw("template_literal")`), so the swc collector treats `Tpl` as a leaf;
//! such literals are simply not recovered (sound).

mod collect;

use std::collections::HashMap;

use rts_hir::ir::{HirExprKind, HirLit};
use rts_hir::{HirExpr, HirFunc, HirStmt, HirType};

use rts_ast::ast::{Item, Statement};

use super::super::class::{build_literal_class, ClassTable, LitMethod};

/// The marker field key prepended to an HIR `Object` node to carry its recovered
/// literal-class name to `lower_object_literal` (which strips it). Chosen so it can
/// never collide with a real JS property name reachable from source.
pub(crate) const LIT_CLASS_MARKER: &str = "__rtsl_class__";

/// The literal-class name a marker carries when the literal contains an UNSUPPORTED
/// member (getter/setter/computed/generator/async method, spread). It is NOT a real
/// class; `lower_object_literal` bails on it so the literal never silently degrades
/// to a partial object (a getter read would otherwise return a wrong `undefined`).
pub(crate) const LIT_UNSUPPORTED: &str = "__rtsl_unsupported__";

/// Recover object-literal methods across the main body + every plain user function,
/// registering each synthesized literal class in `classes` and appending its method
/// `HirFunc`s to `funcs`. Mirrors the template/destructure recovery's per-unit
/// pairing. Runs BEFORE the template/optchain/destructure desugar so its marker is
/// in place before any of those rewrite the object's field VALUES, and before arrow
/// extraction so a literal inside a top-level arrow is recovered in the main body.
pub(crate) fn desugar_obj_methods(
    program: &rts_ast::ast::Program,
    main_body: &mut Vec<HirStmt>,
    funcs: &mut Vec<HirFunc>,
    classes: &mut ClassTable,
) {
    let mut rec = Recovery { classes, new_funcs: Vec::new(), names: HashMap::new() };

    let top_stmts: Vec<&swc_ecma_ast::Stmt> = program
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Statement(Statement::Raw(raw)) => raw.stmt.as_ref(),
            _ => None,
        })
        .collect();
    rec.rewrite_unit(&top_stmts, main_body);

    for it in &program.items {
        if let Item::Function(fdecl) = it {
            let swc_stmts: Vec<&swc_ecma_ast::Stmt> = fdecl
                .body
                .iter()
                .filter_map(|s| match s {
                    Statement::Raw(raw) => raw.stmt.as_ref(),
                })
                .collect();
            // Find the lowered HirFunc by name (skip extracted arrows — none exist
            // yet at this stage). A by-name match is exact: user fn names are unique.
            if let Some(f) = funcs.iter_mut().find(|f| f.name == fdecl.name && !f.is_arrow) {
                rec.rewrite_unit(&swc_stmts, &mut f.body);
            }
        }
    }

    let new = rec.new_funcs;
    funcs.extend(new);
}

/// Per-program recovery state: the class table to register into, the synthesized
/// method funcs to append, and the content-key → class-name de-dup map.
struct Recovery<'a> {
    classes: &'a mut ClassTable,
    new_funcs: Vec<HirFunc>,
    /// content key (fields + method debug) → assigned literal-class name.
    names: HashMap<String, String>,
}

impl Recovery<'_> {
    /// Rewrite one paired unit: collect the swc object literals in document order,
    /// then pre-order-walk the HIR consuming one per HIR `Object` node.
    fn rewrite_unit(&mut self, swc_stmts: &[&swc_ecma_ast::Stmt], hir_stmts: &mut [HirStmt]) {
        let objs = collect::collect_object_lits(swc_stmts);
        let mut cur = Cursor { objs: &objs, next: 0 };
        for s in hir_stmts.iter_mut() {
            self.rewrite_stmt(s, &mut cur);
        }
    }

    fn rewrite_stmt(&mut self, stmt: &mut HirStmt, cur: &mut Cursor) {
        match stmt {
            HirStmt::Expr(e) | HirStmt::Throw(e) => self.rewrite_expr(e, cur),
            HirStmt::Return(opt) => {
                if let Some(e) = opt {
                    self.rewrite_expr(e, cur);
                }
            }
            HirStmt::Let { init: Some(e), .. } => self.rewrite_expr(e, cur),
            HirStmt::Const { init, .. } => self.rewrite_expr(init, cur),
            HirStmt::If { cond, then, else_ } => {
                self.rewrite_expr(cond, cur);
                self.rewrite_stmts(then, cur);
                if let Some(e) = else_ {
                    self.rewrite_stmts(e, cur);
                }
            }
            HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
                self.rewrite_expr(cond, cur);
                self.rewrite_stmts(body, cur);
            }
            HirStmt::For { init, cond, update, body } => {
                if let Some(i) = init {
                    self.rewrite_stmt(i, cur);
                }
                if let Some(c) = cond {
                    self.rewrite_expr(c, cur);
                }
                if let Some(u) = update {
                    self.rewrite_expr(u, cur);
                }
                self.rewrite_stmts(body, cur);
            }
            HirStmt::ForOf { iterable, body, .. } => {
                self.rewrite_expr(iterable, cur);
                self.rewrite_stmts(body, cur);
            }
            HirStmt::ForIn { object, body, .. } => {
                self.rewrite_expr(object, cur);
                self.rewrite_stmts(body, cur);
            }
            HirStmt::Try { body, catch, finally } => {
                self.rewrite_stmts(body, cur);
                if let Some(c) = catch {
                    self.rewrite_stmts(&mut c.body, cur);
                }
                if let Some(f) = finally {
                    self.rewrite_stmts(f, cur);
                }
            }
            HirStmt::Switch { discriminant, cases } => {
                self.rewrite_expr(discriminant, cur);
                for case in cases {
                    if let Some(t) = &mut case.test {
                        self.rewrite_expr(t, cur);
                    }
                    self.rewrite_stmts(&mut case.body, cur);
                }
            }
            HirStmt::Block(b) => self.rewrite_stmts(b, cur),
            HirStmt::Labeled { body, .. } => self.rewrite_stmt(body, cur),
            _ => {}
        }
    }

    fn rewrite_stmts(&mut self, stmts: &mut [HirStmt], cur: &mut Cursor) {
        for s in stmts.iter_mut() {
            self.rewrite_stmt(s, cur);
        }
    }

    /// Pre-order: handle an `Object` node (consume the paired swc object) BEFORE
    /// recursing into its field values, matching swc's pre-order visit so the
    /// positional pairing stays exact across nested literals.
    fn rewrite_expr(&mut self, e: &mut HirExpr, cur: &mut Cursor) {
        if let HirExprKind::Object(fields) = &mut e.kind {
            let swc_obj = cur.advance();
            if let Some(obj) = swc_obj {
                if let Some(marker) = self.recover(obj, fields) {
                    // Prepend the marker field (carries the literal-class name to
                    // `lower_object_literal`, which strips it before the slot fill).
                    fields.insert(0, marker);
                }
            }
        }
        self.recurse_children(e, cur);
    }

    /// Recover the literal-class for `swc_obj` whose field props lowered to `fields`.
    /// Returns the marker field `(LIT_CLASS_MARKER, Str(class))` when the literal is
    /// recoverable, else `None` (no methods, or a hard/unsupported member → bail).
    fn recover(
        &mut self,
        swc_obj: &swc_ecma_ast::ObjectLit,
        fields: &[(String, HirExpr)],
    ) -> Option<(String, HirExpr)> {
        let methods = match collect::recover_methods(swc_obj) {
            collect::Recovered::Plain => return None,
            collect::Recovered::Unsupported => return Some(marker(LIT_UNSUPPORTED)),
            collect::Recovered::Methods(m) => m,
        };
        let field_keys: Vec<String> = fields.iter().map(|(k, _)| k.clone()).collect();
        let key = content_key(&field_keys, &methods);
        let class_name = if let Some(name) = self.names.get(&key) {
            name.clone()
        } else {
            let name = format!("__rtsl_lit_{}", self.names.len());
            // The literal bakes this same global shape-id into slot 0 (interned from
            // the field keys); the literal class MUST share it so `this.field` reads
            // resolve to the same slots.
            let global_shape = crate::shape::intern_global_shape(&field_keys);
            let lit_methods: Vec<LitMethod> = methods
                .iter()
                .map(|m| LitMethod { name: m.name.clone(), function: m.function })
                .collect();
            let (desc, fns) =
                build_literal_class(&name, &field_keys, global_shape, &lit_methods);
            if !self.classes.contains(&name) {
                self.classes.insert(desc);
                self.new_funcs.extend(fns);
            }
            self.names.insert(key, name.clone());
            name
        };
        Some(marker(&class_name))
    }

    fn recurse_children(&mut self, e: &mut HirExpr, cur: &mut Cursor) {
        match &mut e.kind {
            HirExprKind::Bin { lhs, rhs, .. } => {
                self.rewrite_expr(lhs, cur);
                self.rewrite_expr(rhs, cur);
            }
            HirExprKind::Unary { operand, .. } => self.rewrite_expr(operand, cur),
            HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
                self.rewrite_expr(target, cur);
                self.rewrite_expr(value, cur);
            }
            HirExprKind::Call { callee, args } => {
                self.rewrite_expr(callee, cur);
                for a in args {
                    self.rewrite_expr(a, cur);
                }
            }
            HirExprKind::MethodCall { object, args, .. } => {
                self.rewrite_expr(object, cur);
                for a in args {
                    self.rewrite_expr(a, cur);
                }
            }
            HirExprKind::New { args, .. } => {
                for a in args {
                    self.rewrite_expr(a, cur);
                }
            }
            HirExprKind::Member { object, .. } => self.rewrite_expr(object, cur),
            HirExprKind::Index { object, index } => {
                self.rewrite_expr(object, cur);
                self.rewrite_expr(index, cur);
            }
            HirExprKind::Array(elems) => {
                for el in elems {
                    self.rewrite_expr(el, cur);
                }
            }
            HirExprKind::Object(obj_fields) => {
                for (_, v) in obj_fields {
                    self.rewrite_expr(v, cur);
                }
            }
            HirExprKind::Ternary { cond, then, else_ } => {
                self.rewrite_expr(cond, cur);
                self.rewrite_expr(then, cur);
                self.rewrite_expr(else_, cur);
            }
            HirExprKind::Await(inner)
            | HirExprKind::Spread(inner)
            | HirExprKind::Cast { expr: inner, .. }
            | HirExprKind::PreInc(inner)
            | HirExprKind::PreDec(inner)
            | HirExprKind::PostInc(inner)
            | HirExprKind::PostDec(inner) => self.rewrite_expr(inner, cur),
            HirExprKind::Seq(exprs) => {
                for ex in exprs {
                    self.rewrite_expr(ex, cur);
                }
            }
            // An arrow body is a SEPARATE unit (extracted later); its swc objects are
            // not in this unit's collector, so do NOT walk into it here (would
            // desync the positional pairing). Arrows keep no recovery — sound.
            HirExprKind::Arrow { .. } => {}
            HirExprKind::Lit(_) | HirExprKind::Ident(_) | HirExprKind::Raw(_) => {}
        }
    }
}

/// Cursor over the swc object literals of one unit, consumed in document order.
struct Cursor<'a> {
    objs: &'a [swc_ecma_ast::ObjectLit],
    next: usize,
}

impl<'a> Cursor<'a> {
    fn advance(&mut self) -> Option<&'a swc_ecma_ast::ObjectLit> {
        let o = self.objs.get(self.next);
        self.next += 1;
        o
    }
}

/// Build the `(LIT_CLASS_MARKER, Str(name))` HIR field prepended to an object node.
fn marker(class_name: &str) -> (String, HirExpr) {
    (
        LIT_CLASS_MARKER.to_string(),
        HirExpr::new(HirExprKind::Lit(HirLit::Str(class_name.to_string())), HirType::Str),
    )
}

/// A stable content key for de-duplicating literal classes: the ordered field keys
/// plus each method's name and its swc-function debug form (so two literals share a
/// class iff their fields AND method bodies are identical).
fn content_key(fields: &[String], methods: &[collect::RecoveredMethod<'_>]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = write!(s, "f:{}|", fields.join(","));
    for m in methods {
        let _ = write!(s, "m:{}={:?};", m.name, m.function);
    }
    s
}
