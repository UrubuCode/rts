//! P5.8 desugar pass — recover TEMPLATE LITERALS and OPTIONAL CHAINING.
//!
//! rts-hir cannot model either construct yet and we must not modify that crate,
//! so both fall to a structureless [`HirExprKind::Raw`]:
//! - a template `` `a${x}b` `` → `Raw("template_literal")` (NO structure at all);
//! - an optional chain `a?.b` → `Raw("OptChain(OptChainExpr { span: A..B, … })")`
//!   (the Rust `{:?}` of the swc node — which carries its source SPAN).
//!
//! This pass walks the REAL swc AST (the same `rts_ast` items `build_program` is
//! lowering) PAIRED with the HIR it produced, and rewrites every such `Raw`
//! placeholder into ordinary HIR the existing lowerer already runs:
//! - a template becomes a left-associative string `+` chain
//!   (`q0 + ToString(e0) + q1 + …`, seeded so the whole chain is string-typed), so
//!   it reuses the one string-coercing `__rtsadp_add` path — `${5}`→"5",
//!   `${true}`→"true", `${[1,2]}`→"1,2", `${null}`→"null",
//!   `${undefined}`→"undefined" — never a divergent coercion;
//! - an optional chain becomes a nested `Ternary { __rts_is_nullish(recv),
//!   undefined, <access> }` that the lowerer short-circuits to `undefined` at the
//!   first nullish link (see [`optchain`]).
//!
//! Correlation is per UNIT (one statement list at a time): within a single swc
//! statement list and the HIR it lowered to, both walks are deterministic
//! depth-first in source order, every `Tpl` produces exactly one
//! `Raw("template_literal")`, and optional chains additionally carry an exact byte
//! span — so the pairing is unambiguous. Units we do not pair (class methods,
//! extracted arrows) keep their `Raw` placeholders and bail at lowering, exactly
//! as before — never a miscompile.

mod destructure;
mod objmethod;
mod optchain;
mod tpl;

pub(crate) use destructure::desugar_destructure;
pub(crate) use objmethod::{LIT_CLASS_MARKER, LIT_UNSUPPORTED, desugar_obj_methods};
pub(super) use optchain::{OPT_CALL, OPT_GET, OPT_INDEX, OPT_METHOD_CALL};

use std::collections::HashMap;

use rts_hir::ir::{HirArrowBody, HirExprKind};
use rts_hir::{HirExpr, HirStmt};

use rts_ast::ast::{Item, Statement};

/// Rewrite template-literal and optional-chain placeholders in the top-level
/// `main` body and in every plain user function body, pairing each HIR unit with
/// its swc source so correlation is exact and local. Class-method / extracted-arrow
/// bodies are not paired (their placeholders bail at lowering, as before).
///
/// `program` is the freshly parsed AST (`build_program` already parsed it; we take
/// a reference to the same items). `main_body` is the synthesized `__rtsn_main`
/// body; `funcs` are the lowered user functions (looked up by name).
pub(crate) fn desugar(
    program: &rts_ast::ast::Program,
    main_body: &mut Vec<HirStmt>,
    funcs: &mut [rts_hir::HirFunc],
    classes: &super::class::ClassTable,
    lit_fn_bodies: &std::collections::HashMap<String, Vec<swc_ecma_ast::Stmt>>,
) {
    // Collect the swc statements that became the main body, in source order.
    let top_stmts: Vec<&swc_ecma_ast::Stmt> = program
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Statement(Statement::Raw(raw)) => raw.stmt.as_ref(),
            _ => None,
        })
        .collect();
    rewrite_unit(&top_stmts, main_body);

    // Pair each user function declaration with its lowered HirFunc by name.
    for it in &program.items {
        if let Item::Function(fdecl) = it {
            let swc_stmts: Vec<&swc_ecma_ast::Stmt> = fdecl
                .body
                .iter()
                .filter_map(|s| match s {
                    Statement::Raw(raw) => raw.stmt.as_ref(),
                })
                .collect();
            if let Some(f) = funcs
                .iter_mut()
                .find(|f| f.name == fdecl.name && !f.is_arrow)
            {
                rewrite_unit(&swc_stmts, &mut f.body);
            }
        }
    }

    // Pair each CLASS METHOD / accessor body with its synthesized HirFunc. A
    // method body's statements are 1:1 with its swc source (only a leading `this`
    // PARAM is synthesized — no body statement is injected), so positional pairing
    // is exact, exactly like a free function. This is what recovers a template /
    // optional-chain INSIDE a class method (the `rts:test` `Matcher` failure
    // messages are the canonical case). The CONSTRUCTOR is intentionally skipped:
    // its HIR body carries a synthesized prologue (field inits, `super(..)`) ahead
    // of the user statements, so the swc/HIR positional pairing would be offset —
    // a constructor template/chain stays Raw (a later increment; ctors rarely use
    // them, the bundle's does not).
    for it in &program.items {
        let Item::Class(c) = it else { continue };
        let Some(desc) = classes.get(&c.name) else {
            continue;
        };
        for m in &c.members {
            // CONSTRUCTOR: its HIR body carries a synthesized PROLOGUE (field
            // inits, `super(..)`) ahead of the user statements, so the swc/HIR
            // positional pairing is only sound when the prologue itself contains
            // NO template placeholders. Verify by count: the HIR body's
            // `Raw("template_literal")`/`Raw("TaggedTpl(..)")` placeholders must
            // equal the swc ctor body's templates — then every placeholder came
            // from the user statements and document order matches. On mismatch
            // (a field initializer with a template) the ctor stays Raw (honest
            // bail, as before).
            if let rts_ast::ast::ClassMember::Constructor(cd) = m {
                let swc_stmts: Vec<&swc_ecma_ast::Stmt> = cd
                    .body
                    .iter()
                    .filter_map(|s| match s {
                        Statement::Raw(raw) => raw.stmt.as_ref(),
                    })
                    .collect();
                let mut acc = Recovered::default();
                for s in &swc_stmts {
                    tpl::walk_stmt(s, &mut acc);
                }
                if let Some(f) = funcs.iter_mut().find(|f| f.name == desc.ctor) {
                    let dump = format!("{:?}", f.body);
                    let ph = dump.matches("Raw(\"template_literal\")").count()
                        + dump.matches("Raw(\"TaggedTpl(").count();
                    if ph == acc.templates.len() + acc.tagged_templates.len() {
                        rewrite_unit(&swc_stmts, &mut f.body);
                        super::class::rewrite_this_block(&mut f.body);
                    }
                }
                continue;
            }
            let rts_ast::ast::ClassMember::Method(md) = m else {
                continue;
            };
            use rts_ast::ast::MethodRole;
            let synth = match md.role {
                // A STATIC method's synthesized fn lives in `desc.statics`
                // (`__rtsn_static_C_m`), not `desc.methods` — without this arm a
                // template/optional-chain inside a static method stayed Raw.
                MethodRole::Method if md.modifiers.is_static => desc.statics.get(&md.name),
                MethodRole::Method => desc.methods.get(&md.name),
                MethodRole::Getter => desc.accessors.get(&md.name).and_then(|a| a.getter.as_ref()),
                MethodRole::Setter => desc.accessors.get(&md.name).and_then(|a| a.setter.as_ref()),
            };
            let Some(synth) = synth else { continue };
            let swc_stmts: Vec<&swc_ecma_ast::Stmt> = md
                .body
                .iter()
                .filter_map(|s| match s {
                    Statement::Raw(raw) => raw.stmt.as_ref(),
                })
                .collect();
            if let Some(f) = funcs.iter_mut().find(|f| &f.name == synth) {
                rewrite_unit(&swc_stmts, &mut f.body);
                // Template / optional-chain recovery RE-LOWERS the interpolated swc
                // sub-exprs through rts-hir, which re-emits a `this` reference as a
                // fresh `Raw("This(..)")` — but the class `this`-rewrite already ran
                // (during synth, BEFORE desugar). So re-run it here, idempotently, to
                // bind any `this` that re-appeared inside a recovered template/chain
                // in a method body (`` `${this._actual}` `` in `Matcher`). It only
                // touches `Raw("This(..)")` nodes, so an already-`Ident("this")` body
                // is unchanged.
                super::class::rewrite_this_block(&mut f.body);
            }
        }
    }

    // LIT-METHOD bodies (synthesized by the object-literal recovery, incl.
    // nested literals): pair each with the swc body the recovery kept, so a
    // template / OPTIONAL CHAIN inside a literal method/getter/setter recovers
    // exactly like one in a free function (`set value(v) { this.deps?.forEach(…) }`).
    for f in funcs.iter_mut() {
        if let Some(body) = lit_fn_bodies.get(&f.name) {
            let refs: Vec<&swc_ecma_ast::Stmt> = body.iter().collect();
            rewrite_unit(&refs, &mut f.body);
            super::class::rewrite_this_block(&mut f.body);
        }
    }
}

/// Rewrite one paired unit: collect the swc structure of `swc_stmts`, then walk
/// `hir_stmts` consuming placeholders in the same order.
fn rewrite_unit(swc_stmts: &[&swc_ecma_ast::Stmt], hir_stmts: &mut [HirStmt]) {
    let mut acc = Recovered::default();
    for s in swc_stmts {
        tpl::walk_stmt(s, &mut acc);
    }
    let mut cx = Cursor {
        recovered: acc,
        next_template: 0,
        next_tagged: 0,
    };
    cx.rewrite_stmts(hir_stmts);
}

/// The swc structure recovered for one unit.
#[derive(Default)]
struct Recovered {
    /// Every `Tpl` node in document order (matched positionally within the unit).
    templates: Vec<swc_ecma_ast::Tpl>,
    /// Every `TaggedTpl` (`` tag`…` ``) in document order (matched positionally).
    tagged_templates: Vec<swc_ecma_ast::TaggedTpl>,
    /// `byte-span (start,end)` → the `OptChainExpr` node (matched by exact span).
    opt_chains: HashMap<(u32, u32), swc_ecma_ast::OptChainExpr>,
}

/// Walks the HIR of one unit, consuming placeholders in document order.
struct Cursor {
    recovered: Recovered,
    next_template: usize,
    next_tagged: usize,
}

impl Cursor {
    fn rewrite_stmts(&mut self, stmts: &mut [HirStmt]) {
        for s in stmts {
            self.rewrite_stmt(s);
        }
    }

    fn rewrite_stmt(&mut self, stmt: &mut HirStmt) {
        match stmt {
            HirStmt::Expr(e) => self.rewrite_expr(e),
            HirStmt::Return(Some(e)) => self.rewrite_expr(e),
            HirStmt::Throw(e) => self.rewrite_expr(e),
            HirStmt::Let { init: Some(e), .. } => self.rewrite_expr(e),
            HirStmt::Const { init, .. } => self.rewrite_expr(init),
            HirStmt::If { cond, then, else_ } => {
                self.rewrite_expr(cond);
                self.rewrite_stmts(then);
                if let Some(e) = else_ {
                    self.rewrite_stmts(e);
                }
            }
            HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
                self.rewrite_expr(cond);
                self.rewrite_stmts(body);
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    self.rewrite_stmt(i);
                }
                if let Some(c) = cond {
                    self.rewrite_expr(c);
                }
                if let Some(u) = update {
                    self.rewrite_expr(u);
                }
                self.rewrite_stmts(body);
            }
            HirStmt::ForOf { iterable, body, .. } => {
                self.rewrite_expr(iterable);
                self.rewrite_stmts(body);
            }
            HirStmt::ForIn { object, body, .. } => {
                self.rewrite_expr(object);
                self.rewrite_stmts(body);
            }
            HirStmt::Try {
                body,
                catch,
                finally,
            } => {
                self.rewrite_stmts(body);
                if let Some(c) = catch {
                    self.rewrite_stmts(&mut c.body);
                }
                if let Some(f) = finally {
                    self.rewrite_stmts(f);
                }
            }
            HirStmt::Switch {
                discriminant,
                cases,
            } => {
                self.rewrite_expr(discriminant);
                for case in cases {
                    if let Some(t) = &mut case.test {
                        self.rewrite_expr(t);
                    }
                    self.rewrite_stmts(&mut case.body);
                }
            }
            HirStmt::Block(b) => self.rewrite_stmts(b),
            HirStmt::Labeled { body, .. } => self.rewrite_stmt(body),
            _ => {}
        }
    }

    /// Rewrite an expression depth-first. A `Raw` placeholder for a template or
    /// optional chain is replaced from the recovered swc structure; everything
    /// else recurses into its children.
    fn rewrite_expr(&mut self, e: &mut HirExpr) {
        if let HirExprKind::Raw(payload) = &e.kind {
            if payload == "template_literal" {
                let idx = self.next_template;
                self.next_template += 1;
                if let Some(t) = self.recovered.templates.get(idx).cloned() {
                    *e = tpl::build_template(&t);
                }
                return;
            }
            if payload.starts_with("TaggedTpl(") {
                let idx = self.next_tagged;
                self.next_tagged += 1;
                if let Some(tt) = self.recovered.tagged_templates.get(idx).cloned() {
                    // `build_tagged_template` rebuilds the args (incl. nested
                    // templates) directly from swc — like `build_template`, it is
                    // self-contained, so we do NOT recurse (no placeholders remain).
                    *e = tpl::build_tagged_template(&tt);
                }
                return;
            }
            if let Some(span) = optchain::parse_span(payload) {
                if let Some(oc) = self.recovered.opt_chains.get(&span).cloned() {
                    if let Some(built) = optchain::build_opt_chain(&oc) {
                        *e = built;
                    }
                }
                return;
            }
            return;
        }
        self.recurse_children(e);
    }

    /// Recurse into every child expression of a non-placeholder node.
    fn recurse_children(&mut self, e: &mut HirExpr) {
        match &mut e.kind {
            HirExprKind::Bin { lhs, rhs, .. } => {
                self.rewrite_expr(lhs);
                self.rewrite_expr(rhs);
            }
            HirExprKind::Unary { operand, .. } => self.rewrite_expr(operand),
            HirExprKind::Assign { target, value } => {
                self.rewrite_expr(target);
                self.rewrite_expr(value);
            }
            HirExprKind::AssignOp { target, value, .. } => {
                self.rewrite_expr(target);
                self.rewrite_expr(value);
            }
            HirExprKind::Call { callee, args } => {
                self.rewrite_expr(callee);
                for a in args {
                    self.rewrite_expr(a);
                }
            }
            HirExprKind::MethodCall { object, args, .. } => {
                self.rewrite_expr(object);
                for a in args {
                    self.rewrite_expr(a);
                }
            }
            HirExprKind::New { args, .. } => {
                for a in args {
                    self.rewrite_expr(a);
                }
            }
            HirExprKind::Member { object, .. } => self.rewrite_expr(object),
            HirExprKind::Index { object, index } => {
                self.rewrite_expr(object);
                self.rewrite_expr(index);
            }
            HirExprKind::Array(elems) => {
                for el in elems {
                    self.rewrite_expr(el);
                }
            }
            HirExprKind::Object(fields) => {
                for (_, v) in fields {
                    self.rewrite_expr(v);
                }
            }
            HirExprKind::Ternary { cond, then, else_ } => {
                self.rewrite_expr(cond);
                self.rewrite_expr(then);
                self.rewrite_expr(else_);
            }
            HirExprKind::Await(inner)
            | HirExprKind::Spread(inner)
            | HirExprKind::Cast { expr: inner, .. }
            | HirExprKind::PreInc(inner)
            | HirExprKind::PreDec(inner)
            | HirExprKind::PostInc(inner)
            | HirExprKind::PostDec(inner) => self.rewrite_expr(inner),
            HirExprKind::Seq(exprs) => {
                for ex in exprs {
                    self.rewrite_expr(ex);
                }
            }
            HirExprKind::Arrow { body, .. } => match body {
                HirArrowBody::Expr(ex) => self.rewrite_expr(ex),
                HirArrowBody::Block(stmts) => self.rewrite_stmts(stmts),
            },
            HirExprKind::Lit(_) | HirExprKind::Ident(_) | HirExprKind::Raw(_) => {}
        }
    }
}
