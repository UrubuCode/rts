//! SCOPE-AWARE free-variable scan over a function body.
//!
//! `lowering_items` needs one question answered about a generator it is about to
//! hoist out of its enclosing scope: WHICH names does the body take from that
//! scope? The answer becomes the leading parameters of the lifted
//! `__genexpr_N` and the arguments the wrapper left in place passes back — so a
//! name MISSING from the answer is not a missed optimization, it is a name that
//! silently stops resolving after the hoist.
//!
//! The previous scan was FLAT: one `bound` set fed by every `visit_param` /
//! `visit_var_declarator` / `visit_fn_decl` in the whole subtree, nested
//! functions included. A parameter of a NESTED function therefore bound the
//! OUTER name it merely shadows, and the outer name dropped out of the capture
//! list. Measured on a real WhatsApp Web bundle
//! (`WAWebRunInBatches`, `wa2/ext5.js`):
//!
//! ```js
//! var e;                                            // outer scope
//! function s(){ return gen(function*(){
//!     yield 1, yield (e = function (e) { return 1 })  //  ^ param `e` shadows
//! }) }
//! ```
//!
//! `capturas` returned `[]`, the generator was hoisted with no `e` parameter,
//! and lowering ended at `assignment to unbound \`e\``.
//!
//! This module answers the same question with a SCOPE STACK: a binder only
//! covers uses inside the function that introduces it. The result is a superset
//! of the flat answer (a flat `bound` can only over-bind), so the change can add
//! captures and never remove one.

use std::collections::HashSet;

use swc_ecma_ast::{
    ArrowExpr, BlockStmt, CatchClause, Decl, FnExpr, Function, Ident, Pat, VarDeclOrExpr,
};
use swc_ecma_visit::{Visit, VisitWith};

/// Names bound by `pat`, following destructuring (array/object/rest/default).
fn pat_names(p: &Pat, out: &mut HashSet<String>) {
    match p {
        Pat::Ident(bi) => {
            out.insert(bi.id.sym.to_string());
        }
        Pat::Array(a) => a.elems.iter().flatten().for_each(|e| pat_names(e, out)),
        Pat::Rest(r) => pat_names(&r.arg, out),
        Pat::Assign(a) => pat_names(&a.left, out),
        Pat::Object(o) => {
            for prop in &o.props {
                match prop {
                    swc_ecma_ast::ObjectPatProp::KeyValue(kv) => pat_names(&kv.value, out),
                    swc_ecma_ast::ObjectPatProp::Assign(a) => {
                        out.insert(a.key.id.sym.to_string());
                    }
                    swc_ecma_ast::ObjectPatProp::Rest(r) => pat_names(&r.arg, out),
                }
            }
        }
        _ => {}
    }
}

/// Every name DECLARED by one function scope: its own parameters plus the
/// `var`/`let`/`const`/`function`/`class` declarations of its body — stopping at
/// the boundary of any nested function, whose declarations belong to ITS scope.
///
/// A `catch` parameter is deliberately NOT collected here; it is block-scoped and
/// [`FreeScan::visit_catch_clause`] pushes it as its own scope.
#[derive(Default)]
struct Binders(HashSet<String>);

impl Visit for Binders {
    // The scope boundary: declarations below belong to the nested function.
    fn visit_function(&mut self, _: &Function) {}
    fn visit_arrow_expr(&mut self, _: &ArrowExpr) {}

    fn visit_var_declarator(&mut self, d: &swc_ecma_ast::VarDeclarator) {
        pat_names(&d.name, &mut self.0);
        d.visit_children_with(self);
    }
    fn visit_decl(&mut self, d: &Decl) {
        match d {
            Decl::Fn(f) => {
                self.0.insert(f.ident.sym.to_string());
            }
            Decl::Class(c) => {
                self.0.insert(c.ident.sym.to_string());
            }
            _ => {}
        }
        d.visit_children_with(self);
    }
    fn visit_for_head(&mut self, h: &swc_ecma_ast::ForHead) {
        if let swc_ecma_ast::ForHead::Pat(p) = h {
            pat_names(p, &mut self.0);
        }
        h.visit_children_with(self);
    }
    fn visit_for_stmt(&mut self, f: &swc_ecma_ast::ForStmt) {
        if let Some(VarDeclOrExpr::VarDecl(vd)) = &f.init {
            for d in &vd.decls {
                pat_names(&d.name, &mut self.0);
            }
        }
        f.visit_children_with(self);
    }
    // A catch body's `var`s still belong to this function, so we descend — the
    // PARAMETER is what we skip (it is block-scoped).
    fn visit_catch_clause(&mut self, c: &CatchClause) {
        c.body.visit_with(self);
    }
}

/// Bindings introduced by a function scope: `params` + the declarations of `body`.
fn function_scope<'a>(
    params: impl Iterator<Item = &'a Pat>,
    body: Option<&BlockStmt>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    for p in params {
        pat_names(p, &mut out);
    }
    if let Some(b) = body {
        let mut binders = Binders::default();
        b.visit_children_with(&mut binders);
        out.extend(binders.0);
    }
    out
}

/// Collects the identifiers USED in a body that no enclosing scope (up to, and
/// including, the scanned function itself) binds — in order of first use.
struct FreeScan {
    scopes: Vec<HashSet<String>>,
    free: Vec<String>,
    /// Subset of `free` that the body WRITES (`x = …`, `x += …`, `x++`).
    assigned: HashSet<String>,
}

impl FreeScan {
    fn bound(&self, n: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(n))
    }
    fn note(&mut self, n: String) {
        if !self.bound(&n) && !self.free.contains(&n) {
            self.free.push(n);
        }
    }
    fn note_write(&mut self, n: String) {
        if !self.bound(&n) {
            self.assigned.insert(n);
        }
    }
}

impl Visit for FreeScan {
    fn visit_ident(&mut self, i: &Ident) {
        self.note(i.sym.to_string());
    }

    fn visit_function(&mut self, f: &Function) {
        self.scopes
            .push(function_scope(f.params.iter().map(|p| &p.pat), f.body.as_ref()));
        f.visit_children_with(self);
        self.scopes.pop();
    }

    fn visit_arrow_expr(&mut self, a: &ArrowExpr) {
        let body = match a.body.as_ref() {
            swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => Some(b),
            swc_ecma_ast::BlockStmtOrExpr::Expr(_) => None,
        };
        self.scopes.push(function_scope(a.params.iter(), body));
        a.visit_children_with(self);
        self.scopes.pop();
    }

    /// A named function EXPRESSION binds its own name inside its body.
    fn visit_fn_expr(&mut self, fe: &FnExpr) {
        let named = fe.ident.is_some();
        if let Some(id) = &fe.ident {
            self.scopes
                .push(HashSet::from([id.sym.to_string()]));
        }
        fe.function.visit_with(self);
        if named {
            self.scopes.pop();
        }
    }

    fn visit_catch_clause(&mut self, c: &CatchClause) {
        let mut s = HashSet::new();
        if let Some(p) = &c.param {
            pat_names(p, &mut s);
        }
        self.scopes.push(s);
        c.body.visit_with(self);
        self.scopes.pop();
    }

    fn visit_assign_expr(&mut self, a: &swc_ecma_ast::AssignExpr) {
        if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(id)) =
            &a.left
        {
            self.note_write(id.id.sym.to_string());
        }
        a.visit_children_with(self);
    }

    fn visit_update_expr(&mut self, u: &swc_ecma_ast::UpdateExpr) {
        if let swc_ecma_ast::Expr::Ident(id) = u.arg.as_ref() {
            self.note_write(id.sym.to_string());
        }
        u.visit_children_with(self);
    }
}

/// The FREE names of `f`'s body, in order of first use, excluding `f`'s own
/// bindings (parameters + body declarations) and every nested scope's bindings.
///
/// The caller still filters the result against the names it knows are reachable
/// after the hoist (ambient globals, sibling declarations lifted alongside).
pub fn free_names(f: &Function) -> Vec<String> {
    scan(f).free
}

/// The free names of `f`'s body that `f` also WRITES.
///
/// A capture is passed to the lifted function BY VALUE, so a write to one is
/// invisible to the scope the name came from. The caller uses this to refuse the
/// hoist rather than emit a snapshot that silently drops the write.
pub fn free_assigned_names(f: &Function) -> HashSet<String> {
    scan(f).assigned
}

fn scan(f: &Function) -> FreeScan {
    let mut s = FreeScan {
        scopes: vec![function_scope(
            f.params.iter().map(|p| &p.pat),
            f.body.as_ref(),
        )],
        free: Vec::new(),
        assigned: HashSet::new(),
    };
    if let Some(b) = &f.body {
        b.visit_with(&mut s);
    }
    s
}
