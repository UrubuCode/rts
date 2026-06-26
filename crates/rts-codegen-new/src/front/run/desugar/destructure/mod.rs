//! P5.11 destructuring desugar — array `[a, b, ...rest]` and object
//! `{x, y: z, w = 5}` patterns in `let`/`const`, `for-of` bindings, and function
//! parameters.
//!
//! rts-hir flattens every binding pattern to a single name `"_"` (see
//! `extract_swc_pat_name`), dropping the pattern but KEEPING the fully-lowered
//! initializer. So, exactly like the P5.8 template/optional-chain recovery, this
//! pass re-reads the swc AST PAIRED with the HIR and rewrites each flattened binding
//! into ordinary element/property reads the existing lowerer already runs:
//!
//! - `const [a, b] = SRC` → `{ const __t = SRC; const a = __t[0]; const b = __t[1]; }`
//!   (the temp is elided when SRC is already a bare identifier — `const a = SRC[0]`);
//! - `const {x, y: z} = SRC` → `const x = SRC.x; const z = SRC.y;`;
//! - a default `= d` → `(acc === undefined) ? d : acc`; a rest `...r` → `SRC.slice(i)`;
//!   a hole `,` skips the index;
//! - `for (const [k, v] of it) body` → the loop binds a temp `__e`, and the body is
//!   prefixed with `const [k, v] = __e;` (itself expanded);
//! - `function f([a, b]) {…}` → the param is renamed `__p0` and the body prefixed
//!   with `const [a, b] = __p0;`.
//!
//! Everything outside the modeled subset BAILS TOTALLY (the original `"_"` binding
//! is left in place and bails at lowering): a nested pattern element, an object
//! rest, a computed object key, or a non-identifier / non-literal source whose temp
//! has no proven shape. The soundness floor holds — never a silently wrong binding.

mod builders;
mod pat;

use rts_hir::ir::HirExprKind;
use rts_hir::{HirExpr, HirFunc, HirStmt, HirType};

use rts_ast::ast::{Item, Statement};

/// A monotonic counter for synthesized temp names, unique per program run.
struct Gen {
    n: usize,
}

impl Gen {
    fn fresh(&mut self, kind: &str) -> String {
        self.n += 1;
        format!("__rtsd_{kind}_{}", self.n)
    }
}

/// Rewrite destructuring in the main body, every plain user function body, and
/// every function/arrow PARAMETER (recovered from a fresh swc re-parse of `src`,
/// since rts-ast does not carry param patterns). Runs AFTER the template/optchain
/// desugar so any template inside a destructured initializer is already real HIR.
pub(crate) fn desugar_destructure(
    src: &str,
    program: &rts_ast::ast::Program,
    main_body: &mut Vec<HirStmt>,
    funcs: &mut Vec<HirFunc>,
) {
    let mut g = Gen { n: 0 };

    // ---- bodies: pair each swc statement list with its HIR statement list ----
    let top_stmts: Vec<&swc_ecma_ast::Stmt> = program
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Statement(Statement::Raw(raw)) => raw.stmt.as_ref(),
            _ => None,
        })
        .collect();
    rewrite_stmts(&top_stmts, main_body, &mut g);

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
                rewrite_stmts(&swc_stmts, &mut f.body, &mut g);
            }
        }
    }

    // ---- parameters: recover the swc param patterns from a fresh re-parse ----
    params::rewrite_params(src, funcs, &mut g);
}

mod params;

/// Rewrite a paired (swc, HIR) statement list IN PLACE: expand each destructuring
/// `let`/`const` / `for-of`, recursing into nested blocks/control flow. The swc and
/// HIR lists are in 1:1 source order (rts-hir lowers statements one-for-one, except
/// a multi-declarator `var` which becomes a `Block` — handled by recursion).
fn rewrite_stmts(swc_stmts: &[&swc_ecma_ast::Stmt], hir_stmts: &mut Vec<HirStmt>, g: &mut Gen) {
    // Walk HIR + swc together by index. Because a destructuring rewrite can REPLACE
    // one HIR statement with several (wrapped in a Block, so the index alignment to
    // swc is preserved), we collect into a fresh vector.
    let mut out: Vec<HirStmt> = Vec::with_capacity(hir_stmts.len());
    let taken = std::mem::take(hir_stmts);
    for (i, mut stmt) in taken.into_iter().enumerate() {
        let swc = swc_stmts.get(i).copied();
        rewrite_one(&mut stmt, swc, g, &mut out);
    }
    *hir_stmts = out;
}

/// Rewrite one HIR statement, pushing the result(s) onto `out`. Most statements are
/// pushed unchanged after recursing into their nested bodies; a destructuring
/// `let`/`const` or `for-of` is expanded.
fn rewrite_one(
    stmt: &mut HirStmt,
    swc: Option<&swc_ecma_ast::Stmt>,
    g: &mut Gen,
    out: &mut Vec<HirStmt>,
) {
    match stmt {
        // A flattened destructuring binding has name "_" — try to expand it from the
        // paired swc declarator pattern. If the swc is not a destructuring var decl,
        // or the pattern is unsupported, the original statement is kept (and bails).
        HirStmt::Const { name, init, .. } if name == "_" => {
            if let Some(expanded) = try_expand_let(swc, Some(init), g) {
                out.push(HirStmt::Block(expanded));
                return;
            }
        }
        HirStmt::Let { name, init, .. } if name == "_" => {
            if let Some(expanded) = try_expand_let(swc, init.as_ref(), g) {
                out.push(HirStmt::Block(expanded));
                return;
            }
        }
        // `for (const <pat> of it) body`: the HIR binding is "_" for a pattern.
        HirStmt::ForOf { binding, body, .. } if binding == "_" => {
            // Recurse into the ORIGINAL body FIRST (swc/HIR still index-aligned), THEN
            // rename the binding + prefix `const <pat> = <elem>;` — so the prefix's
            // extra statements never break the body's positional pairing.
            let inner_swc = for_of_body_stmts(swc);
            rewrite_stmts(&inner_swc, body, g);
            if try_expand_for_of(swc, binding, body, g).is_some() {
                out.push(stmt.clone());
                return;
            }
        }
        _ => {}
    }
    // Not a (successful) destructuring rewrite: recurse into nested bodies, then push.
    recurse_nested(stmt, swc, g);
    out.push(stmt.clone());
}

/// Recurse into the nested statement lists of a control-flow statement so a
/// destructuring binding inside an `if`/`while`/`for`/block/try is expanded too.
fn recurse_nested(stmt: &mut HirStmt, swc: Option<&swc_ecma_ast::Stmt>, g: &mut Gen) {
    match stmt {
        HirStmt::If { then, else_, .. } => {
            let (then_swc, else_swc) = if_branch_stmts(swc);
            rewrite_stmts(&then_swc, then, g);
            if let Some(e) = else_ {
                rewrite_stmts(&else_swc, e, g);
            }
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            let inner = loop_body_stmts(swc);
            rewrite_stmts(&inner, body, g);
        }
        HirStmt::For { body, .. } => {
            let inner = loop_body_stmts(swc);
            rewrite_stmts(&inner, body, g);
        }
        HirStmt::ForOf { body, .. } | HirStmt::ForIn { body, .. } => {
            let inner = for_of_body_stmts(swc);
            rewrite_stmts(&inner, body, g);
        }
        HirStmt::Block(b) => {
            let inner = block_stmts(swc);
            rewrite_stmts(&inner, b, g);
        }
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            let (try_swc, catch_swc, fin_swc) = try_stmts(swc);
            rewrite_stmts(&try_swc, body, g);
            if let Some(c) = catch {
                rewrite_stmts(&catch_swc, &mut c.body, g);
            }
            if let Some(f) = finally {
                rewrite_stmts(&fin_swc, f, g);
            }
        }
        // Switch / labeled: nested destructuring there is rare; we conservatively do
        // not pair them (any destructuring inside keeps its "_" and bails — sound).
        _ => {}
    }
}

/// Expand a destructuring `let`/`const` whose swc declarator carries an array/object
/// pattern, off the already-lowered `init` HIR. Returns the expansion statement list
/// (the binding lets), or `None` to bail.
fn try_expand_let(
    swc: Option<&swc_ecma_ast::Stmt>,
    init: Option<&HirExpr>,
    g: &mut Gen,
) -> Option<Vec<HirStmt>> {
    let decl = var_decl_pat(swc?)?;
    let init = init?;
    let (src_name, prelude) = source_local(init, g);
    let body = expand_pat(decl, &src_name, g)?;
    let mut out = prelude;
    out.extend(body);
    Some(out)
}

/// Expand a destructuring `for-of` binding: rename the HIR binding to a fresh element
/// temp and PREFIX the loop body with `const <pat> = <temp>;` (itself expanded).
/// Returns `Some(())` on success (binding + body mutated), `None` to bail.
fn try_expand_for_of(
    swc: Option<&swc_ecma_ast::Stmt>,
    binding: &mut String,
    body: &mut Vec<HirStmt>,
    g: &mut Gen,
) -> Option<()> {
    let pat = for_of_pat(swc?)?;
    let elem = g.fresh("e");
    let prefix = expand_pat(pat, &elem, g)?;
    *binding = elem;
    let mut new_body = prefix;
    new_body.append(body);
    *body = new_body;
    Some(())
}

/// Build the binding statements for `pat` reading off source local `src`,
/// recursing into nested patterns via fresh temps (`g`).
fn expand_pat(pat: &swc_ecma_ast::Pat, src: &str, g: &mut Gen) -> Option<Vec<HirStmt>> {
    pat::expand_pat(src, pat, g)
}

/// Decide the SOURCE local a pattern reads from. A bare-identifier init is used
/// directly (its proven shape is reused, no temp); any other init is bound to a
/// fresh temp via a `const __rtsd_t_N = <init>` prelude so the element/property reads
/// resolve against the temp (which carries a proven shape only when the init is an
/// array/object literal — otherwise the reads bail, which is the intended behavior).
fn source_local(init: &HirExpr, g: &mut Gen) -> (String, Vec<HirStmt>) {
    if let HirExprKind::Ident(name) = &init.kind {
        return (name.clone(), Vec::new());
    }
    let tmp = g.fresh("t");
    let bind = const_bind_init(&tmp, init.clone());
    (tmp, vec![bind])
}

/// `const tmp = init;` carrying the init's own type (so an array/object-literal init
/// keeps its shape-recording path in the lowerer's `let`).
fn const_bind_init(name: &str, init: HirExpr) -> HirStmt {
    HirStmt::Const {
        name: name.to_string(),
        ty: HirType::Any,
        init,
    }
}

// ---------------------------------------------------------------------------
// swc structure accessors — recover the pattern / nested statement lists.
// ---------------------------------------------------------------------------

/// The single-declarator array/object `Pat` of a `let`/`const`/`var` statement, or
/// `None` if not a destructuring var decl.
fn var_decl_pat(stmt: &swc_ecma_ast::Stmt) -> Option<&swc_ecma_ast::Pat> {
    let swc_ecma_ast::Stmt::Decl(swc_ecma_ast::Decl::Var(vd)) = stmt else {
        return None;
    };
    let d = vd.decls.first()?;
    match &d.name {
        p @ (swc_ecma_ast::Pat::Array(_) | swc_ecma_ast::Pat::Object(_)) => Some(p),
        _ => None,
    }
}

/// The for-of head binding `Pat` (array/object), or `None` (a plain-ident binding or
/// a non-for-of statement).
fn for_of_pat(stmt: &swc_ecma_ast::Stmt) -> Option<&swc_ecma_ast::Pat> {
    let swc_ecma_ast::Stmt::ForOf(fo) = stmt else {
        return None;
    };
    let pat = match &fo.left {
        swc_ecma_ast::ForHead::VarDecl(vd) => &vd.decls.first()?.name,
        swc_ecma_ast::ForHead::Pat(p) => p.as_ref(),
        swc_ecma_ast::ForHead::UsingDecl(_) => return None,
    };
    match pat {
        swc_ecma_ast::Pat::Array(_) | swc_ecma_ast::Pat::Object(_) => Some(pat),
        _ => None,
    }
}

fn for_of_body_stmts(stmt: Option<&swc_ecma_ast::Stmt>) -> Vec<&swc_ecma_ast::Stmt> {
    match stmt {
        Some(swc_ecma_ast::Stmt::ForOf(fo)) => block_or_single(&fo.body),
        Some(swc_ecma_ast::Stmt::ForIn(fi)) => block_or_single(&fi.body),
        _ => Vec::new(),
    }
}

fn loop_body_stmts(stmt: Option<&swc_ecma_ast::Stmt>) -> Vec<&swc_ecma_ast::Stmt> {
    match stmt {
        Some(swc_ecma_ast::Stmt::While(w)) => block_or_single(&w.body),
        Some(swc_ecma_ast::Stmt::DoWhile(w)) => block_or_single(&w.body),
        Some(swc_ecma_ast::Stmt::For(f)) => block_or_single(&f.body),
        _ => Vec::new(),
    }
}

fn if_branch_stmts(
    stmt: Option<&swc_ecma_ast::Stmt>,
) -> (Vec<&swc_ecma_ast::Stmt>, Vec<&swc_ecma_ast::Stmt>) {
    match stmt {
        Some(swc_ecma_ast::Stmt::If(i)) => {
            let then = block_or_single(&i.cons);
            let els = i.alt.as_deref().map(block_or_single).unwrap_or_default();
            (then, els)
        }
        _ => (Vec::new(), Vec::new()),
    }
}

fn block_stmts(stmt: Option<&swc_ecma_ast::Stmt>) -> Vec<&swc_ecma_ast::Stmt> {
    match stmt {
        Some(swc_ecma_ast::Stmt::Block(b)) => b.stmts.iter().collect(),
        _ => Vec::new(),
    }
}

fn try_stmts(
    stmt: Option<&swc_ecma_ast::Stmt>,
) -> (
    Vec<&swc_ecma_ast::Stmt>,
    Vec<&swc_ecma_ast::Stmt>,
    Vec<&swc_ecma_ast::Stmt>,
) {
    match stmt {
        Some(swc_ecma_ast::Stmt::Try(t)) => {
            let body = t.block.stmts.iter().collect();
            let catch = t
                .handler
                .as_ref()
                .map(|h| h.body.stmts.iter().collect())
                .unwrap_or_default();
            let fin = t
                .finalizer
                .as_ref()
                .map(|f| f.stmts.iter().collect())
                .unwrap_or_default();
            (body, catch, fin)
        }
        _ => (Vec::new(), Vec::new(), Vec::new()),
    }
}

/// A loop/if body is either a `{ ... }` block (its statements) or a single statement.
fn block_or_single(stmt: &swc_ecma_ast::Stmt) -> Vec<&swc_ecma_ast::Stmt> {
    match stmt {
        swc_ecma_ast::Stmt::Block(b) => b.stmts.iter().collect(),
        single => vec![single],
    }
}
