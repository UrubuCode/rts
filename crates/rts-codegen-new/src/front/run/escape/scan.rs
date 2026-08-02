//! The per-LOCAL half of Tier-0 escape analysis: one linear pass over the
//! function body deciding whether a `const p = new C(...)` local's value ever
//! leaves the function.
//!
//! ## The whitelist
//!
//! Exactly ONE use of the local is legal: **a READ of a declared field**,
//! `p.<field>` (optionally through casts — `(p as any).x`). Every other occurrence
//! of the identifier bails. That single rule is what makes the pass safe by
//! construction rather than by enumeration: a construct nobody thought of is a
//! plain identifier occurrence somewhere, and a plain identifier occurrence bails.
//!
//! ## The bail list, as implemented
//!
//! Structural, checked before the walk:
//!
//! * the class has no [`super::ScalarCtor`] (see that module's own bail list:
//!   inheritance, accessors, a non-trivial ctor body, non-numeric fields, …);
//! * the argument count is not the constructor's exact parameter count;
//! * the name is DECLARED more than once in the function (this scan is name-based,
//!   like `super::super::floatscan`, so two scopes sharing a spelling must bail —
//!   for float promotion a name collision only costs speed, here it would be a
//!   miscompile);
//! * the name is CAPTURED — `funcval::scan::arrow_free_idents`, the engine's
//!   existing capture detection, reused as the bail signal rather than reimplemented.
//!
//! From the walk:
//!
//! * **returned** — `return p` is a bare identifier;
//! * **stored** — `o.f = p`, `a[i] = p`, `[…p…]`, `{ k: p }`: all bare identifiers
//!   in value position;
//! * **passed to ANY call** — `f(p)`, `p.m()`, `o.m(p)`, `new D(p)`, a spread
//!   `f(...p)`, a tagged template. `p.m()` matters most: the receiver is passed as
//!   `this`, so a constructor or method that does anything beyond field stores
//!   makes the object visible to code this pass cannot see. It is a bail because
//!   the identifier is the `MethodCall`'s object, not a `Member`;
//! * **captured by a closure** — any occurrence inside an `Arrow` body, whatever
//!   the position. Even `p.x` inside an arrow bails: the closure captures the
//!   binding, and the scalar fields are `Variable`s of the ENCLOSING function that
//!   the closure's own frame cannot reach;
//! * **`===`/`==` compared** (and `!=`/`!==`) — identity. Two structurally equal
//!   objects are different values; a register tuple has no identity to compare.
//!   Bare identifier, so the generic rule catches it;
//! * **used as a collection key** — `m.set(p, v)`, `s.add(p)`, `o[p]`: a call
//!   argument or an index operand, both bare identifiers;
//! * **thrown** — `throw p`;
//! * **dynamic property add / delete** — `delete p.x`, `p.z = 1` for an undeclared
//!   `z`. Both are WRITES through the local, and this increment bails on ANY field
//!   write (see below), so both are covered by the same rule;
//! * **`instanceof` / `typeof` / `Object.keys(p)` / `JSON.stringify(p)` /
//!   template interpolation / `String(p)`** — every one of them is either a bare
//!   identifier operand or a call argument. None needs its own rule; each is listed
//!   so a reader can confirm it was considered rather than missed;
//! * **any FIELD WRITE**, `p.x = v` / `p.x += v` / `p.x++`. Not because a write is
//!   unsound in principle — a `Variable` is exactly the right home for one — but
//!   because the field's `Repr` is taken from its INITIALIZER at the construction
//!   site, and a later assignment of a different representation has no join point
//!   to widen at. Allowing writes needs a per-slot `Repr` join first, which is the
//!   `RTS_OPTIMIZATION.md` item that sits immediately before this one. Listed as a
//!   deliberate, documented restriction, not an oversight;
//! * **`HirStmt::Raw` / `HirExprKind::Raw`** whose text mentions the name. Raw
//!   carries un-modelled source text; if the name appears in it, we do not know
//!   what happens to it.
//!
//! ## Casts
//!
//! Both the whitelist and the write detection look THROUGH `Cast`, so
//! `(p as any).x` reads and `(p as any).x = 1` bails. This is
//! `super::super::floatscan`'s bug applied as a rule: a scan that stops at a
//! `Cast` cannot see the value reaching the site, and the annotation that tells a
//! reader "this is fine" is exactly what hides that it may not be.
//!
//! Re-binding is a use, not a pass-through: `const q = p;` is a bare identifier in
//! the initializer, so it bails. (The same shape `floatscan` had to learn to
//! follow, resolved here in the other direction — following it would mean alias
//! analysis, which Tier 0 does not do.)

use std::collections::{HashMap, HashSet};

use rts_hir::ir::{HirArrowBody, HirExpr, HirExprKind, HirStmt};

use super::super::class::ClassTable;

/// Locals in `body` that hold a scalar-replaceable, non-escaping `new C(...)`:
/// local name → class name. The lowering consults this at the `let`/`const` site
/// and at every `p.<field>` read.
pub(in crate::front::run) fn scalar_locals(
    body: &[HirStmt],
    classes: &ClassTable,
    captures: &HashMap<String, Vec<String>>,
) -> HashMap<String, String> {
    // Cheap structural pass first: collect the candidate `const p = new C(a, b)`
    // bindings, then count how many times each name is DECLARED anywhere.
    let mut candidates: Vec<(String, String)> = Vec::new();
    let mut decl_counts: HashMap<String, usize> = HashMap::new();
    collect(body, &mut candidates, &mut decl_counts, classes);
    if candidates.is_empty() {
        return HashMap::new();
    }

    // CAPTURE: reuse the engine's existing arrow-capture detection as the bail
    // signal, exactly as `RTS_OPTIMIZATION.md` §5 Tier 4.1 requires — a second
    // capture analysis living here would be a second thing to keep true. The
    // walker's own `Arrow`-is-opaque rule below is what makes the coverage
    // complete (that helper's statement walk does not descend into every
    // statement form); this one is the named signal.
    // CAPTURE, post-lift. `arrow_free_idents` finds names read free inside an
    // `Arrow` NODE — but by the time this scan runs the arrows have already been
    // LIFTED into their own `HirFunc`s, so there is no `Arrow` node left to look
    // inside and the helper answers "nothing is captured". A fixture caught it:
    // `const p = new Pt(7,8); const g = () => p.x + p.y; return g();` scalar-
    // replaced `p` and then the lifted arrow could not find it —
    // `ReferenceError: p is not defined`, a miscompile rather than a slowdown.
    //
    // The signal that survives lifting is the CAPTURES map the lowering already
    // carries: one entry per lifted function listing the outer names it closed
    // over. A name in ANY of those lists is captured by construction.
    let mut captured = super::super::funcval::scan::arrow_free_idents(body);
    for names in captures.values() {
        captured.extend(names.iter().cloned());
    }

    let mut out = HashMap::new();
    for (name, class) in candidates {
        if decl_counts.get(&name).copied().unwrap_or(0) != 1 || captured.contains(&name) {
            continue;
        }
        let Some(desc) = classes.get(&class) else {
            continue;
        };
        let fields: HashSet<&str> = desc.fields.iter().map(String::as_str).collect();
        let mut c = Checker {
            name: name.as_str(),
            fields: &fields,
            ok: true,
        };
        c.stmts(body);
        if c.ok {
            out.insert(name, class);
        }
    }
    out
}

/// Collect `const p = new C(args)` candidates and count every binding occurrence
/// of every name (so a name declared twice can be refused).
fn collect(
    stmts: &[HirStmt],
    candidates: &mut Vec<(String, String)>,
    decls: &mut HashMap<String, usize>,
    classes: &ClassTable,
) {
    for s in stmts {
        match s {
            HirStmt::Let { name, init, .. } => {
                *decls.entry(name.clone()).or_default() += 1;
                if let Some(e) = init {
                    note_candidate(name, e, candidates, classes);
                }
            }
            HirStmt::Const { name, init, .. } => {
                *decls.entry(name.clone()).or_default() += 1;
                note_candidate(name, init, candidates, classes);
            }
            HirStmt::ForOf { binding, .. } | HirStmt::ForIn { binding, .. } => {
                *decls.entry(binding.clone()).or_default() += 1;
            }
            HirStmt::Try { catch, .. } => {
                if let Some(c) = catch {
                    if let Some(b) = &c.binding {
                        *decls.entry(b.clone()).or_default() += 1;
                    }
                }
            }
            _ => {}
        }
        for_each_child_block(s, &mut |b| collect(b, candidates, decls, classes));
    }
}

fn note_candidate(
    name: &str,
    init: &HirExpr,
    candidates: &mut Vec<(String, String)>,
    classes: &ClassTable,
) {
    let HirExprKind::New { class, args } = &init.kind else {
        return;
    };
    let Some(desc) = classes.get(class) else {
        return;
    };
    let Some(recipe) = &desc.scalar_ctor else {
        return;
    };
    // Exact positional arity only — the recipe carries no default/rest logic, and
    // `marshal_call_args` (which owns that logic) is precisely what the replay
    // path skips.
    if args.len() != recipe.params.len() {
        return;
    }
    candidates.push((name.to_string(), class.clone()));
}

/// Run `f` over every statement list nested inside `stmt`. Mirrors the helper in
/// `super::super::floatscan` — same traversal, kept local so a change to one
/// pre-scan's traversal cannot silently retune the other.
fn for_each_child_block(stmt: &HirStmt, f: &mut impl FnMut(&[HirStmt])) {
    match stmt {
        HirStmt::If { then, else_, .. } => {
            f(then);
            if let Some(e) = else_ {
                f(e);
            }
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => f(body),
        HirStmt::For { init, body, .. } => {
            if let Some(i) = init {
                f(std::slice::from_ref(&**i));
            }
            f(body);
        }
        HirStmt::ForOf { body, .. } | HirStmt::ForIn { body, .. } => f(body),
        HirStmt::Block(body) => f(body),
        HirStmt::Labeled { body, .. } => f(std::slice::from_ref(&**body)),
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            f(body);
            if let Some(c) = catch {
                f(&c.body);
            }
            if let Some(fin) = finally {
                f(fin);
            }
        }
        HirStmt::Switch { cases, .. } => {
            for c in cases {
                f(&c.body);
            }
        }
        _ => {}
    }
}

/// The use-walk for ONE candidate. `ok` latches to `false` on the first bail and
/// is never set back — there is no "recovered" state.
struct Checker<'n> {
    name: &'n str,
    fields: &'n HashSet<&'n str>,
    ok: bool,
}

impl Checker<'_> {
    fn stmts(&mut self, stmts: &[HirStmt]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &HirStmt) {
        if !self.ok {
            return;
        }
        match s {
            HirStmt::Expr(e) | HirStmt::Throw(e) | HirStmt::Const { init: e, .. } => self.expr(e),
            HirStmt::Return(e) => {
                if let Some(e) = e {
                    self.expr(e);
                }
            }
            HirStmt::Let { init, .. } => {
                if let Some(e) = init {
                    self.expr(e);
                }
            }
            HirStmt::If { cond, .. } => self.expr(cond),
            HirStmt::While { cond, .. } | HirStmt::DoWhile { cond, .. } => self.expr(cond),
            HirStmt::For { cond, update, .. } => {
                if let Some(c) = cond {
                    self.expr(c);
                }
                if let Some(u) = update {
                    self.expr(u);
                }
            }
            HirStmt::ForOf { iterable, .. } => self.expr(iterable),
            HirStmt::ForIn { object, .. } => self.expr(object),
            HirStmt::Switch { discriminant, cases } => {
                self.expr(discriminant);
                for c in cases {
                    if let Some(t) = &c.test {
                        self.expr(t);
                    }
                }
            }
            // Un-modelled source text. If it mentions the name we cannot say what
            // it does with it, so bail rather than guess.
            HirStmt::Raw(txt) => {
                if txt.contains(self.name) {
                    self.ok = false;
                }
            }
            HirStmt::Block(_)
            | HirStmt::Try { .. }
            | HirStmt::Labeled { .. }
            | HirStmt::Break(_)
            | HirStmt::Continue(_) => {}
        }
        // Nested statement lists, for every statement form that has them.
        for_each_child_block(s, &mut |body| {
            for st in body {
                self.stmt(st);
            }
        });
    }

    /// The ONE legal use: `p.<declared field>` in read position, seen through casts.
    fn is_field_read(&self, e: &HirExpr) -> bool {
        let HirExprKind::Member { object, prop } = &e.kind else {
            return false;
        };
        self.is_the_local(object) && self.fields.contains(prop.as_str())
    }

    /// `e` is the candidate identifier, possibly wrapped in casts.
    fn is_the_local(&self, e: &HirExpr) -> bool {
        matches!(&strip_casts(e).kind, HirExprKind::Ident(n) if n == self.name)
    }

    fn expr(&mut self, e: &HirExpr) {
        if !self.ok {
            return;
        }
        if self.is_field_read(e) {
            // Whitelisted. Do NOT recurse — the object subtree is the identifier
            // itself, which the generic rule would (correctly, for any other
            // position) bail on.
            return;
        }
        match &e.kind {
            HirExprKind::Ident(n) => {
                if n == self.name {
                    self.ok = false;
                }
            }
            HirExprKind::Assign { target, value } => {
                self.target(target);
                self.expr(value);
            }
            HirExprKind::AssignOp { target, value, .. } => {
                self.target(target);
                self.expr(value);
            }
            HirExprKind::PreInc(t)
            | HirExprKind::PreDec(t)
            | HirExprKind::PostInc(t)
            | HirExprKind::PostDec(t) => self.target(t),
            // `delete p.x` — a dynamic delete on the local. `target` bails on a
            // member whose object is the local, which is exactly the rule wanted.
            HirExprKind::Unary { op, operand } if matches!(op, rts_hir::ir::HirUnOp::Delete) => {
                self.target(operand)
            }
            // A CLOSURE body is opaque to the scalar fields: they are `Variable`s
            // of this function's frame. Any occurrence of the name inside, in any
            // position (even `p.x`), is a capture and bails.
            HirExprKind::Arrow { body, .. } => match body {
                HirArrowBody::Expr(inner) => self.opaque_expr(inner),
                HirArrowBody::Block(stmts) => {
                    for s in stmts {
                        self.opaque_stmt(s);
                    }
                }
            },
            HirExprKind::Raw(txt) => {
                if txt.contains(self.name) {
                    self.ok = false;
                }
            }
            _ => for_each_child_expr(e, &mut |c| self.expr(c)),
        }
    }

    /// An ASSIGNMENT target. Three shapes bail immediately, because the generic
    /// read rule would otherwise accept them:
    ///   * `p = …`      — rebinding the local (the fields would be stale);
    ///   * `p.f = …`    — a field write (see the module doc: deferred, needs a
    ///                     per-slot `Repr` join);
    ///   * `p[i] = …`   — a computed write, i.e. a dynamic add.
    /// Anything else is walked as an ordinary expression, so `o.f = p` still bails
    /// through the bare-identifier rule on the VALUE side.
    fn target(&mut self, t: &HirExpr) {
        if !self.ok {
            return;
        }
        if self.is_the_local(t) {
            self.ok = false;
            return;
        }
        match &t.kind {
            HirExprKind::Member { object, .. } | HirExprKind::Index { object, .. } => {
                if self.is_the_local(object) {
                    self.ok = false;
                    return;
                }
            }
            _ => {}
        }
        self.expr(t);
    }

    /// Inside a closure: ANY occurrence of the name bails, whitelist disabled.
    fn opaque_expr(&mut self, e: &HirExpr) {
        if !self.ok {
            return;
        }
        if let HirExprKind::Ident(n) = &e.kind {
            if n == self.name {
                self.ok = false;
                return;
            }
        }
        if let HirExprKind::Raw(txt) = &e.kind {
            if txt.contains(self.name) {
                self.ok = false;
                return;
            }
        }
        if let HirExprKind::Arrow { body, .. } = &e.kind {
            match body {
                HirArrowBody::Expr(inner) => self.opaque_expr(inner),
                HirArrowBody::Block(stmts) => {
                    for s in stmts {
                        self.opaque_stmt(s);
                    }
                }
            }
            return;
        }
        for_each_child_expr(e, &mut |c| self.opaque_expr(c));
    }

    fn opaque_stmt(&mut self, s: &HirStmt) {
        if !self.ok {
            return;
        }
        for_each_expr_in_stmt(s, &mut |e| self.opaque_expr(e));
        for_each_child_block(s, &mut |body| {
            for st in body {
                self.opaque_stmt(st);
            }
        });
    }
}

/// Strip `x as T` wrappers. A cast is a type ASSERTION — the runtime word is
/// unchanged — so it never changes WHICH value a site touches.
pub(super) fn strip_casts(e: &HirExpr) -> &HirExpr {
    let mut cur = e;
    while let HirExprKind::Cast { expr, .. } = &cur.kind {
        cur = expr;
    }
    cur
}

/// Every direct child EXPRESSION of `e`. Exhaustive by construction: the `_` arm
/// is only reached by leaf kinds, and a kind added to the HIR later that this misses
/// can only make the walk visit FEWER nodes — which is why the whitelist, not this
/// helper, is where safety lives. (`Raw` is handled by the callers, which read its
/// text; it has no child expressions.)
fn for_each_child_expr(e: &HirExpr, f: &mut impl FnMut(&HirExpr)) {
    match &e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        HirExprKind::Unary { operand, .. } => f(operand),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            f(target);
            f(value);
        }
        HirExprKind::Call { callee, args } => {
            f(callee);
            for a in args {
                f(a);
            }
        }
        HirExprKind::MethodCall { object, args, .. } => {
            f(object);
            for a in args {
                f(a);
            }
        }
        HirExprKind::New { args, .. } => {
            for a in args {
                f(a);
            }
        }
        HirExprKind::Member { object, .. } => f(object),
        HirExprKind::Index { object, index } => {
            f(object);
            f(index);
        }
        HirExprKind::Array(items) | HirExprKind::Seq(items) => {
            for i in items {
                f(i);
            }
        }
        HirExprKind::Object(props) => {
            for (_, v) in props {
                f(v);
            }
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            f(cond);
            f(then);
            f(else_);
        }
        HirExprKind::Await(inner)
        | HirExprKind::Cast { expr: inner, .. }
        | HirExprKind::Spread(inner)
        | HirExprKind::PreInc(inner)
        | HirExprKind::PreDec(inner)
        | HirExprKind::PostInc(inner)
        | HirExprKind::PostDec(inner) => f(inner),
        HirExprKind::Arrow { body, .. } => match body {
            HirArrowBody::Expr(inner) => f(inner),
            HirArrowBody::Block(_) => {}
        },
        HirExprKind::Lit(_) | HirExprKind::Ident(_) | HirExprKind::Raw(_) => {}
    }
}

/// Every EXPRESSION directly held by `s` (not those inside its nested blocks).
fn for_each_expr_in_stmt(s: &HirStmt, f: &mut impl FnMut(&HirExpr)) {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) | HirStmt::Const { init: e, .. } => f(e),
        HirStmt::Return(e) => {
            if let Some(e) = e {
                f(e);
            }
        }
        HirStmt::Let { init, .. } => {
            if let Some(e) = init {
                f(e);
            }
        }
        HirStmt::If { cond, .. } | HirStmt::While { cond, .. } | HirStmt::DoWhile { cond, .. } => {
            f(cond)
        }
        HirStmt::For { cond, update, .. } => {
            if let Some(c) = cond {
                f(c);
            }
            if let Some(u) = update {
                f(u);
            }
        }
        HirStmt::ForOf { iterable, .. } => f(iterable),
        HirStmt::ForIn { object, .. } => f(object),
        HirStmt::Switch { discriminant, cases } => {
            f(discriminant);
            for c in cases {
                if let Some(t) = &c.test {
                    f(t);
                }
            }
        }
        _ => {}
    }
}
