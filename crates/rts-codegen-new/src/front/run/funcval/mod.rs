//! First-class function VALUES — arrow extraction + capture analysis (P4.6).
//!
//! Top-level `const f = (x) => …` is ALREADY hoisted by the parser into a named
//! top-level `HirFunc` (`f`), so `f` lives in `sigs` and a direct call `f(x)`
//! takes the native fast path unchanged. What is NOT hoisted is an INLINE arrow
//! used as a value — an argument (`apply((x) => x + 1, 9)`) or a returned arrow
//! (`return (x) => x * x`). Those stay `HirExprKind::Arrow` nodes.
//!
//! This module runs ONE pre-pass over the program (every top-level function body
//! + the synthesized main body) that, for each NON-CAPTURING inline arrow,
//! synthesizes a fresh top-level `HirFunc` (`__rtsn_arrow_N`) and rewrites the
//! `Arrow` node in place to an `Ident("__rtsn_arrow_N")`. The expr lowering then
//! sees an identifier that resolves to a user function and REIFIES it into a
//! `TAG_FUNCTION` PolyValue (see [`super::expr`] / [`super::call`]).
//!
//! ## Capture rule (P5.7 — capture BY VALUE, sound or bail)
//!
//! An arrow is NON-CAPTURING iff every free identifier in its body is one of its
//! own params, a global (`console`), or the name of a top-level function. Such an
//! arrow is lifted to a plain top-level function (no env).
//!
//! A CAPTURING arrow (a free ident that is an outer LOCAL) is lifted to a CLOSURE
//! (P5.7): its captured free vars are PREPENDED as leading params and the ordered
//! capture list is recorded so the reify site snapshots each captured local's
//! CURRENT value into a fresh env array and the closure thunk reads them back.
//! This is capture-BY-VALUE (a snapshot at closure-creation), which matches JS
//! only when the captured value does not change between capture and call. To stay
//! SOUND we BAIL (leave the arrow → the lowering reports `expression arrow`) when:
//!
//! - a free ident is not a simple capturable local (a `this`, an unknown name);
//! - the closure ASSIGNS a captured var (mutable capture — the write must be
//!   visible to the outer scope, which by-value cannot do);
//! - a captured var is REASSIGNED anywhere in the enclosing function (conservative:
//!   a `let` mutated after capture would make the by-value snapshot observably
//!   stale). A capture that is never reassigned in its enclosing function is
//!   effectively const for the closure's lifetime → by-value is correct.
//!
//! `this`/async/generator arrows are likewise left to bail.

pub mod scan;
pub(crate) use scan::arrow_decl_names as fn_decl_names;

use std::collections::{HashMap, HashSet};

use rts_hir::ir::{HirArrowBody, HirExprKind};
use rts_hir::{HirExpr, HirFunc, HirParam, HirStmt, HirType};

use scan::{
    arrow_assigned_names, arrow_decl_names, arrow_free_idents, collect_free_stmt, declared_names,
    mutated_names,
};

/// MODULE-LEVEL MUTABLE GLOBALS (epic #195). Compute the set of top-level `let`
/// variables that are WRITTEN from inside SOME function (as a free variable — not
/// the function's own param/local). These are promoted to runtime CELLS: every
/// access (top-level + the function) goes through `__RTS_FN_NS_GC_GCELL_GET/SET`
/// by the returned compile-time id, sidestepping the by-value capture limitation.
///
/// The rts:test harness's `print` helper (`let __rtsCapturedOutput = "";
/// function print(v) { __rtsCapturedOutput += v + "\n"; }`) is the canonical case.
/// `const` is excluded (immutable → never written). A name shadowed by a param or
/// a local `let` inside the writing function is NOT counted (that write is local).
pub fn module_globals(
    funcs: &[HirFunc],
    main: &HirFunc,
    force: &HashSet<String>,
) -> HashMap<String, u32> {
    // Top-level `let`/`const` names declared directly in main (ordered, deduped).
    // `const` is included because a FORCE-promoted read-only global (a prelude
    // singleton like `console`, accessed from inside user functions) is a `const`
    // — it is never written, so the written-free promotion below never catches it;
    // only `force` does. A `const` not in `force` still won't promote (the
    // written-free test can't fire on it), so plain user `const`s are unaffected.
    let mut top: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    // Top-level bindings whose initializer is a NON-LITERAL (a call/member/etc. that
    // yields a RUNTIME value with identity — e.g. `const c = atomic.i64_new(0)`). A
    // literal init (`let p = "hi-"`) is re-materializable / by-value-capturable, so a
    // read-only closure capture handles it WITHOUT a cell; a runtime-value init read
    // from a plain function cannot be re-materialized and MUST be a shared cell.
    let mut runtime_init: HashSet<String> = HashSet::new();
    for s in &main.body {
        let (name, init) = match s {
            HirStmt::Let { name, init, .. } => (name, init.as_ref()),
            HirStmt::Const { name, init, .. } => (name, Some(init)),
            _ => continue,
        };
        if seen.insert(name.clone()) {
            top.push(name.clone());
            if init.is_some_and(|e| !matches!(e.kind, HirExprKind::Lit(_))) {
                runtime_init.insert(name.clone());
            }
        }
    }
    if top.is_empty() {
        return HashMap::new();
    }
    // Names written OR read (free) inside SOME function: minus that fn's own params
    // + declared locals (which shadow an outer name of the same spelling). A
    // top-level binding merely READ from a function must ALSO be a cell — a `const
    // counter = atomic.i64_new(0)` read inside `function tally(){…counter…}` holds a
    // RUNTIME value that cannot be re-materialized (the call has identity/side
    // effects), so the function must read the SAME value via the cell, not bail
    // unbound. (A numeric const pays a cell load, accepted for correctness.)
    let mut written_free: HashSet<String> = HashSet::new();
    let mut read_free: HashSet<String> = HashSet::new();
    for f in funcs {
        let mut bound: HashSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
        bound.extend(declared_names(&f.body));
        for n in mutated_names(&f.body) {
            if !bound.contains(&n) {
                written_free.insert(n);
            }
        }
        let mut reads: HashSet<String> = HashSet::new();
        let mut scope = bound.clone();
        for s in &f.body {
            collect_free_stmt(s, &mut scope, &mut reads);
        }
        read_free.extend(reads);
    }
    let mut map = HashMap::new();
    let mut id = 0u32;
    for name in top {
        // Promote a top-level `let`/`const` to a runtime CELL when it is WRITTEN free
        // from a function (the classic #195 mutable case) OR force-promoted, OR when
        // it is READ free from a function AND its init is a runtime value (the
        // read-only-shared-handle case, `const c = atomic.i64_new(0)`). A read-only
        // LITERAL is left to the by-value closure-capture path (don't disturb it).
        let promote = written_free.contains(&name)
            || force.contains(&name)
            || (read_free.contains(&name) && runtime_init.contains(&name));
        if promote {
            map.insert(name, id);
            id += 1;
        }
    }
    map
}

/// The gcell ids that are NEVER WRITTEN after their top-level initializer — i.e.
/// module globals promoted to a cell only because a function READS them
/// (`const PER_WORKER = N / W` used inside a worker fn), not because anything
/// mutates them.
///
/// Their value is fixed once `__rtsn_main`'s initializer has run, so a function
/// may read the cell ONCE at entry and reuse the value for every later read in
/// that body — instead of a `GCELL_GET` extern call per access. That matters
/// enormously in a hot loop: a loop bound read from a module const was costing a
/// call **per iteration** (plus a boxed generic compare, since the cell yields
/// `Tagged`), which measured 2.2x slower than the same loop over a local and, with
/// several threads all hammering the one shared cell, turned parallel speedup
/// NEGATIVE.
///
/// A cell is considered mutable — and so excluded — when any function assigns it
/// free, or when `main` assigns it anywhere after the declaration. Both are
/// conservative: an unrecognised write shape keeps the cell out of this set and
/// therefore on the always-reload path.
pub fn immutable_gcells(
    funcs: &[HirFunc],
    main: &HirFunc,
    gcells: &HashMap<String, u32>,
) -> std::collections::HashSet<u32> {
    let mut written: HashSet<String> = HashSet::new();
    // Written free from any function (the same test `module_globals` promotes on).
    for f in funcs {
        let mut bound: HashSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
        bound.extend(declared_names(&f.body));
        for n in mutated_names(&f.body) {
            if !bound.contains(&n) {
                written.insert(n);
            }
        }
    }
    // Assigned in `main` too: the DECLARATION itself is not a write (that is the
    // initializer), but a later `x = …` is.
    let declared_in_main: HashSet<String> = main
        .body
        .iter()
        .filter_map(|s| match s {
            HirStmt::Let { name, .. } | HirStmt::Const { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    for n in mutated_names(&main.body) {
        if declared_in_main.contains(&n) {
            written.insert(n);
        }
    }
    gcells
        .iter()
        .filter(|(name, _)| !written.contains(*name))
        .map(|(_, id)| *id)
        .collect()
}

/// Rename every `Ident(from)` to `to` across `stmts` (a synthesized closure body
/// whose captured `this` param must not literally be named `this`). Walks the
/// same expr/stmt surface the extraction handles; an unvisited exotic node keeps
/// the old name and lowers to an unbound-ident bail (honest, never a wrong bind).
fn rename_ident_stmts(stmts: &mut [HirStmt], from: &str, to: &str) {
    for s in stmts {
        rename_ident_stmt(s, from, to);
    }
}

fn rename_ident_stmt(s: &mut HirStmt, from: &str, to: &str) {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => rename_ident_expr(e, from, to),
        HirStmt::Return(Some(e)) => rename_ident_expr(e, from, to),
        HirStmt::Let { init: Some(e), .. } => rename_ident_expr(e, from, to),
        HirStmt::Const { init, .. } => rename_ident_expr(init, from, to),
        HirStmt::If { cond, then, else_ } => {
            rename_ident_expr(cond, from, to);
            rename_ident_stmts(then, from, to);
            if let Some(e) = else_ {
                rename_ident_stmts(e, from, to);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            rename_ident_expr(cond, from, to);
            rename_ident_stmts(body, from, to);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                rename_ident_stmt(i, from, to);
            }
            if let Some(c) = cond {
                rename_ident_expr(c, from, to);
            }
            if let Some(u) = update {
                rename_ident_expr(u, from, to);
            }
            rename_ident_stmts(body, from, to);
        }
        HirStmt::ForOf { iterable, body, .. } => {
            rename_ident_expr(iterable, from, to);
            rename_ident_stmts(body, from, to);
        }
        HirStmt::ForIn { object, body, .. } => {
            rename_ident_expr(object, from, to);
            rename_ident_stmts(body, from, to);
        }
        HirStmt::Block(b) => rename_ident_stmts(b, from, to),
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            rename_ident_stmts(body, from, to);
            if let Some(c) = catch {
                rename_ident_stmts(&mut c.body, from, to);
            }
            if let Some(f) = finally {
                rename_ident_stmts(f, from, to);
            }
        }
        _ => {}
    }
}

fn rename_ident_expr(e: &mut HirExpr, from: &str, to: &str) {
    if let HirExprKind::Ident(n) = &mut e.kind {
        if n == from {
            *n = to.to_string();
        }
        return;
    }
    match &mut e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => {
            rename_ident_expr(lhs, from, to);
            rename_ident_expr(rhs, from, to);
        }
        HirExprKind::Unary { operand, .. } => rename_ident_expr(operand, from, to),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            rename_ident_expr(target, from, to);
            rename_ident_expr(value, from, to);
        }
        HirExprKind::Call { callee, args } => {
            rename_ident_expr(callee, from, to);
            args.iter_mut().for_each(|a| rename_ident_expr(a, from, to));
        }
        HirExprKind::MethodCall { object, args, .. } => {
            rename_ident_expr(object, from, to);
            args.iter_mut().for_each(|a| rename_ident_expr(a, from, to));
        }
        HirExprKind::Member { object, .. } => rename_ident_expr(object, from, to),
        HirExprKind::Index { object, index } => {
            rename_ident_expr(object, from, to);
            rename_ident_expr(index, from, to);
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            rename_ident_expr(cond, from, to);
            rename_ident_expr(then, from, to);
            rename_ident_expr(else_, from, to);
        }
        HirExprKind::Array(elems) => elems
            .iter_mut()
            .for_each(|x| rename_ident_expr(x, from, to)),
        HirExprKind::Object(fields) => fields
            .iter_mut()
            .for_each(|(_, v)| rename_ident_expr(v, from, to)),
        HirExprKind::New { args, .. } => {
            args.iter_mut().for_each(|a| rename_ident_expr(a, from, to));
        }
        HirExprKind::Await(inner) | HirExprKind::Spread(inner) => {
            rename_ident_expr(inner, from, to);
        }
        HirExprKind::Seq(items) => items
            .iter_mut()
            .for_each(|x| rename_ident_expr(x, from, to)),
        // A NESTED arrow/fn-expression: the renamed name (a named fn-expr's
        // self-name) is visible inside its body too — descend, unless one of the
        // inner params SHADOWS it. Without this, `function w(n) { return () =>
        // w(n-1); }` kept the inner `w` unrenamed → an unknown free ident → the
        // outer extraction bailed ("expression arrow").
        HirExprKind::Arrow {
            params,
            body,
            self_name,
            ..
        } => {
            let shadowed =
                params.iter().any(|p| p.name == from) || self_name.as_deref() == Some(from);
            if !shadowed {
                match body {
                    rts_hir::ir::HirArrowBody::Expr(inner) => rename_ident_expr(inner, from, to),
                    rts_hir::ir::HirArrowBody::Block(stmts) => rename_ident_stmts(stmts, from, to),
                }
            }
        }
        _ => {}
    }
}

/// Top-level singleton-instance globals — every top-level `const X = …` whose
/// value has a STATICALLY KNOWN class — mapped to that class. DATA-DRIVEN: the
/// engine NAMES nothing (no `"console"` allow-list); it matches SHAPES, so any
/// prelude OR user singleton is covered uniformly. Two shapes qualify:
///
///  - `const X = new Y()` — the class is the ctor (`const console = new Console()`,
///    `const app = new App()`);
///  - `const X = f(…)` where `f` is a BUILTIN IMPORT whose spec ts-signature names
///    a registered class as its return (`const s = createSocket('udp4')` →
///    `createSocket(type: object): Socket`). The class comes from the Registry's
///    own data — the same resolution `global_instance_class` does for a receiver.
///
/// Why this exists: such a `const` resolved as a plain top-level local is INVISIBLE
/// inside a user `function f() { console.log(..) }` (a function reads only
/// locals/params, gcells, and global constants) → "unbound identifier". Two facts
/// must survive into every scope: (a) the VALUE — fixed by promoting the name to a
/// runtime CELL (gcell #195), so every scope reads the one shared instance; (b) the
/// static CLASS — a plain `new Y()` local records `Y` in `local_classes`, but the
/// gcell erases that, so we carry `name → Y` here for `static_instance_class` to
/// recover (the receiver class for `console.log`-style dispatch in any scope).
///
/// ONLY singletons actually REFERENCED from inside a function (or arrow) are
/// promoted — a `const x = new C()` used purely at the top level stays an ordinary
/// local (promoting every singleton would needlessly route plain top-level method
/// calls through a cell and mis-dispatch user singletons, e.g. `const p = new
/// Point(); p.parseValue()`). Returns `name → class` for the promoted set.
pub fn singleton_instance_globals(
    funcs: &[HirFunc],
    main: &HirFunc,
    builtins: &std::collections::HashMap<String, (String, String)>,
) -> std::collections::HashMap<String, String> {
    let mut candidates: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for s in &main.body {
        let HirStmt::Const { name, init, .. } = s else {
            continue;
        };
        match &init.kind {
            rts_hir::ir::HirExprKind::New { class, .. } => {
                candidates.insert(name.clone(), class.clone());
            }
            rts_hir::ir::HirExprKind::Call { callee, args } => {
                if let rts_hir::ir::HirExprKind::Ident(fname) = &callee.kind
                    && let Some((ns, member)) = builtins.get(fname)
                    && let Some(class) = super::registry::namespace_member(ns, member, args.len())
                        .and_then(|c| c.ret_class)
                        .filter(|c| super::registry::has_class(c))
                {
                    candidates.insert(name.clone(), class);
                }
            }
            _ => {}
        }
    }
    if candidates.is_empty() {
        return candidates;
    }
    // Names READ FREE inside some function body OR some arrow in main (the actual
    // reachability condition that needs a shared cell). A function's own params /
    // locals shadow an outer singleton of the same name and don't count.
    let mut referenced: HashSet<String> = HashSet::new();
    for f in funcs {
        let mut bound: HashSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
        bound.extend(declared_names(&f.body));
        for s in &f.body {
            collect_free_stmt(s, &mut bound, &mut referenced);
        }
    }
    referenced.extend(arrow_free_idents(&main.body));
    candidates.retain(|name, _| referenced.contains(name));
    candidates
}

/// Top-level `let`s that must be promoted to runtime CELLs because a closure
/// CAPTURES them AND they are mutated at top level (so a by-value snapshot would be
/// observably stale — `let out=""; out+=…; describe(…, ()=>expect(out)…)`). Such a
/// var, kept as a plain local, makes its capturing arrow BAIL (`mutated` guard in
/// `try_extract`); as a cell it reads live and the arrow lifts cleanly. Computed on
/// the PRE-extraction HIR (arrows still present). Function-written top lets are
/// already cells via [`module_globals`]; this adds the main-mutated-and-captured set.
pub fn captured_mutated_top_lets(funcs: &[HirFunc], main: &HirFunc) -> HashSet<String> {
    // Top-level `let` names in main.
    let top: HashSet<String> = main
        .body
        .iter()
        .filter_map(|s| match s {
            HirStmt::Let { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    if top.is_empty() {
        return HashSet::new();
    }
    // Names some arrow body reads (main + every function body).
    let mut in_arrows = arrow_free_idents(&main.body);
    for f in funcs {
        in_arrows.extend(arrow_free_idents(&f.body));
    }
    // Names mutated at the top level (`out += …`). A top let mutated only inside a
    // function is ALREADY a cell via `module_globals`; this targets top-level writes.
    let mutated_top = mutated_names(&main.body);
    top.into_iter()
        .filter(|n| in_arrows.contains(n) && mutated_top.contains(n))
        .collect()
}

/// Language globals always considered "not a capture" — value singletons plus the
/// PRIMORDIAL/builtin global NAMES an arrow body may reference (resolved at
/// lowering, never a capturable outer local). Critically includes `String`: the
/// template-literal desugar wraps each `${expr}` in `String(expr)`, so an arrow
/// containing a template (`() => `${x}``) reads `String` as a free ident — without
/// it here the arrow would be misjudged a capture of an unknown name and BAIL
/// (`expression arrow`). (A non-primordial like `console` is a gcell singleton,
/// already non-capture via `ctx.module_globals`.)
const GLOBALS: &[&str] = &[
    "undefined",
    "Infinity",
    "NaN",
    "globalThis",
    // PRIMORDIAL wrapper/constructor + spec global functions callable from an
    // arrow. Non-primordial globals (Date/JSON/URL/…) are NOT listed — both
    // call sites also consult `registry::has_class`/`has_namespace`, which
    // covers every registered class data-driven.
    "String",
    "Number",
    "Boolean",
    "Object",
    "Array",
    "Function",
    "Symbol",
    "Math",
    "RegExp",
    "Promise",
    "Error",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    // `getPointer(fn)` — an ENGINE intrinsic (the C-ABI code address of a top-level
    // fn, for `thread.spawn`/callbacks). Globally accessible, never captured, so an
    // arrow/hoisted fn that calls it (`test(() => thread.spawn(getPointer(w), n))`)
    // must NOT treat `getPointer` as an un-capturable free ident → "expression arrow".
    "getPointer",
];

/// The result of the arrow-extraction pre-pass: the synthesized top-level
/// functions to append, plus the ordered capture list of each CLOSURE (synthesized
/// fn name → captured outer-local names, in the order they were prepended as
/// leading params). A non-capturing synthesized fn has no entry here.
pub struct ExtractResult {
    pub funcs: Vec<HirFunc>,
    pub captures: HashMap<String, Vec<String>>,
    /// FUNCTION-LOCAL CELLS (#195): function name → the set of its LOCAL names that
    /// became per-invocation cells (a closure in that function captures AND mutates
    /// them). Keyed by BOTH the declaring function (where the `let` allocates the
    /// cell) AND the synthesized closure (where the captured leading param holds the
    /// handle), so each side routes reads/writes through the cell. See [`super::cell`].
    pub cells: HashMap<String, HashSet<String>>,
}

/// Extract every inline arrow used as a value from `funcs` + `main` into fresh
/// top-level `HirFunc`s, rewriting each in place to an `Ident` of the synthesized
/// name. A non-capturing arrow becomes a plain function; a capturing arrow becomes
/// a CLOSURE (captured vars prepended as leading params + recorded in `captures`).
/// Returns the synthesized functions + the per-closure capture lists.
pub fn extract_arrows(
    funcs: &mut Vec<HirFunc>,
    main: &mut HirFunc,
    ambient_fns: &HashSet<String>,
    class_names: &HashSet<String>,
    module_globals: &HashSet<String>,
    arrow_ns: &str,
    lit_fn_units: &HashMap<String, String>,
) -> ExtractResult {
    let mut top_level: HashSet<String> = funcs.iter().map(|f| f.name.clone()).collect();
    top_level.insert(main.name.clone());
    // AMBIENT prelude functions (the `rts:test` `describe`/`test`/`expect`, the
    // primordial `.ts` helpers) are not in THIS program's `funcs` yet (they merge in
    // later), but a user callback arrow that references one is NOT capturing an outer
    // LOCAL — it is a free reference to a known top-level function. Seed them so such
    // an arrow lifts to a plain function instead of being misjudged an unsound
    // capture and left to bail (`expression arrow`).
    top_level.extend(ambient_fns.iter().cloned());
    // CLASS names (ambient `.ts` collections like `Set`/`Map`, user classes, the
    // synthesized builtin parents): a free reference to one inside an arrow
    // (`xs.filter(v => v instanceof Set)`, `new C()`) is NOT a captured local — it
    // is a known class. Seed them so such an arrow lifts instead of bailing.
    top_level.extend(class_names.iter().cloned());

    let mut ctx = Ctx {
        top_level,
        module_globals: module_globals.clone(),
        synthesized: Vec::new(),
        captures: HashMap::new(),
        cells: HashMap::new(),
        current_fn: String::new(),
        current_fn_lets: HashSet::new(),
        current_fn_arrow_decls: HashSet::new(),
        self_binding: None,
        counter: 0,
        arrow_ns: arrow_ns.to_string(),
    };

    // Rewrite arrows inside every existing function body and the main body. Each
    // function's params are in-scope for its body; the set of names REASSIGNED
    // anywhere in that body decides which captures are sound (by-value) vs bail.
    for f in funcs.iter_mut() {
        let params: HashSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
        let mutated = mutated_names(&f.body);
        ctx.current_fn = f.name.clone();
        ctx.current_fn_lets = declared_locals(&f.body);
        ctx.current_fn_arrow_decls = arrow_decl_names(&f.body);
        ctx.rewrite_block(&mut f.body, &params, &mutated);
    }
    let main_params: HashSet<String> = main.params.iter().map(|p| p.name.clone()).collect();
    let main_mutated = mutated_names(&main.body);
    ctx.current_fn = main.name.clone();
    ctx.current_fn_lets = declared_locals(&main.body);
    ctx.current_fn_arrow_decls = arrow_decl_names(&main.body);
    ctx.rewrite_block(&mut main.body, &main_params, &main_mutated);

    // The synthesized arrow bodies may THEMSELVES contain arrows; rewrite those
    // too (a fixpoint over a growing list — each new function's own params are in
    // scope for its body).
    let mut i = 0;
    while i < ctx.synthesized.len() {
        let mut f = ctx.synthesized[i].clone();
        let params: HashSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
        let mutated = mutated_names(&f.body);
        ctx.current_fn = f.name.clone();
        ctx.current_fn_lets = declared_locals(&f.body);
        ctx.current_fn_arrow_decls = arrow_decl_names(&f.body);
        ctx.rewrite_block(&mut f.body, &params, &mutated);
        ctx.synthesized[i] = f;
        i += 1;
    }

    // P5.7: a `let g = (x) => x*k` at MAIN scope is hoisted by the PARSER into a
    // top-level `HirFunc g` (not an inline `Arrow` node), so the rewrite above
    // never sees it — `k` would be an unbound free var. Detect such hoisted
    // ARROW-functions that capture a main-scope local and convert them to closures
    // (captures prepended as leading params + recorded), so a direct call `g(4)`
    // builds the env from the call-site locals (handled in `lower_call`).
    hoist_captures(funcs, main, &mut ctx);

    // LIT-METHOD captures: a literal method whose body references a LOCAL of
    // the fn that BUILDS the literal (`{ next() { return i++; } }` inside
    // `[Symbol.iterator]() { let i = 0; … }`) captures it through the SAME
    // reify machinery closures use — the tail-slot reify at the build site
    // snapshots the constructing fn's live locals. A capture the lit method
    // MUTATES becomes a CELL of both sides.
    lit_method_captures(funcs, main, lit_fn_units, &mut ctx);

    ExtractResult {
        funcs: ctx.synthesized,
        captures: ctx.captures,
        cells: ctx.cells,
    }
}

/// Convert hoisted top-level functions that capture a main-scope local into
/// closures.
///
/// The PARSER hoists `let g = (x) => …` (and a `function g(){…}`) to a top-level
/// `HirFunc g`; its body's free idents that are main-scope locals (not its params,
/// not globals, not other top-level fns) are lexical CAPTURES — correct JS
/// semantics for both forms. We prepend them as leading params and record the
/// ordered list, applying the SAME by-value soundness rule (no body-assign of a
/// capture, no main-scope reassignment of a captured var). A function whose only
/// free non-globals are NOT main-scope locals is left untouched (a genuine
/// unbound-identifier bail at lowering).
/// Resolve LIT-METHOD free idents against the CONSTRUCTING fn's locals and
/// convert them to captures (leading params + `ctx.captures`), so the
/// tail-slot reify at the literal build site snapshots them (a mutated one is
/// a CELL of both the constructing fn and the lit method). Iterates to a
/// fixpoint so a nested literal (unit = another lit method) resolves after its
/// own unit gained leading capture params.
fn lit_method_captures(
    funcs: &mut Vec<HirFunc>,
    main: &HirFunc,
    lit_fn_units: &HashMap<String, String>,
    ctx: &mut Ctx,
) {
    let main_name = main.name.clone();
    for _round in 0..4 {
        let mut changed = false;
        // Snapshot each unit's (params + declared locals + mutated set) — the
        // borrow on `funcs` must end before we mutate the lit fns.
        let unit_info: HashMap<String, (HashSet<String>, HashSet<String>, HashSet<String>)> = {
            let mut m = HashMap::new();
            for u in lit_fn_units.values() {
                if m.contains_key(u) {
                    continue;
                }
                let (params, lets, muts) = if *u == main_name {
                    (
                        main.params.iter().map(|p| p.name.clone()).collect(),
                        declared_locals(&main.body),
                        mutated_names(&main.body),
                    )
                } else if let Some(f) = funcs.iter().find(|f| f.name == *u) {
                    (
                        f.params.iter().map(|p| p.name.clone()).collect(),
                        declared_locals(&f.body),
                        mutated_names(&f.body),
                    )
                } else {
                    continue;
                };
                m.insert(u.clone(), (params, lets, muts));
            }
            m
        };
        for i in 0..funcs.len() {
            let name = funcs[i].name.clone();
            if ctx.captures.contains_key(&name) {
                continue; // already resolved (previous round).
            }
            let Some(unit) = lit_fn_units.get(&name) else {
                continue;
            };
            if std::env::var_os("RTS_DEBUG_LITCAP").is_some() {
                eprintln!("[litcap] fn={name} unit={unit}");
            }
            let Some((u_params, u_lets, u_muts)) = unit_info.get(unit) else {
                if std::env::var_os("RTS_DEBUG_LITCAP").is_some() {
                    let names: Vec<&str> = funcs
                        .iter()
                        .filter(|f| f.name.contains("lit_0"))
                        .map(|f| f.name.as_str())
                        .collect();
                    eprintln!(
                        "[litcap] NO unit_info for {unit}; lit funcs={names:?}; keys={:?}",
                        lit_fn_units
                    );
                }
                continue;
            };
            let param_names: HashSet<String> =
                funcs[i].params.iter().map(|p| p.name.clone()).collect();
            let mut free = HashSet::new();
            let mut bound = param_names.clone();
            for s in &funcs[i].body {
                collect_free_stmt(s, &mut bound, &mut free);
            }
            let body_assigns = mutated_names(&funcs[i].body);
            let mut captures: Vec<String> = Vec::new();
            let mut cell_caps: Vec<String> = Vec::new();
            let mut ok = true;
            for id in &free {
                if param_names.contains(id)
                    || GLOBALS.contains(&id.as_str())
                    || ctx.top_level.contains(id)
                    || ctx.module_globals.contains(id)
                    || id == "this"
                {
                    continue;
                }
                let in_unit = u_params.contains(id) || u_lets.contains(id);
                if !in_unit {
                    if super::registry::has_namespace(id) || super::registry::has_class(id) {
                        continue;
                    }
                    ok = false; // unresolvable — leave to bail honestly.
                    break;
                }
                let unit_cell = ctx.cells.get(unit).is_some_and(|s| s.contains(id));
                if body_assigns.contains(id) || u_muts.contains(id) || unit_cell {
                    // A mutated / already-cell capture must be a CELL declared
                    // by a `let` of the unit (a mutated unit PARAM can't host
                    // the cell — bail honestly).
                    if !u_lets.contains(id) && !unit_cell {
                        ok = false;
                        break;
                    }
                    cell_caps.push(id.clone());
                }
                captures.push(id.clone());
            }
            if std::env::var_os("RTS_DEBUG_LITCAP").is_some() {
                eprintln!("[litcap] free={free:?} ok={ok} caps={captures:?} u_lets={u_lets:?}");
            }
            if !ok || captures.is_empty() {
                continue;
            }
            captures.sort();
            for c in &cell_caps {
                ctx.cells.entry(unit.clone()).or_default().insert(c.clone());
                ctx.cells.entry(name.clone()).or_default().insert(c.clone());
            }
            let mut new_params: Vec<HirParam> = captures
                .iter()
                .map(|n| HirParam {
                    name: n.clone(),
                    ty: HirType::Any,
                    variadic: false,
                    has_default: false,
                    optional: false,
                    default_expr: None,
                })
                .collect();
            new_params.extend(funcs[i].params.iter().cloned());
            funcs[i].params = new_params;
            ctx.captures.insert(name, captures);
            changed = true;
        }
        if !changed {
            break;
        }
    }
}

fn hoist_captures(funcs: &mut [HirFunc], main: &HirFunc, ctx: &mut Ctx) {
    // Main-scope locals = the `let`/`const` names declared directly in main's body
    // (the only outer scope a hoisted function can capture).
    let main_locals = declared_locals(&main.body);
    let main_mutated = mutated_names(&main.body);

    for f in funcs.iter_mut() {
        // Already-prepended closures (a synthesized arrow handled above) are skipped
        // via the captures map; a fresh hoisted function has no entry yet.
        if ctx.captures.contains_key(&f.name) {
            continue;
        }
        let param_names: HashSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
        let mut free = HashSet::new();
        let mut bound = param_names.clone();
        for s in &f.body {
            collect_free_stmt(s, &mut bound, &mut free);
        }
        let body_assigns = mutated_names(&f.body);

        let mut captures: Vec<String> = Vec::new();
        let mut sound = true;
        for id in &free {
            if param_names.contains(id)
                || GLOBALS.contains(&id.as_str())
                || ctx.top_level.contains(id)
                || ctx.module_globals.contains(id)
                || super::registry::has_namespace(id)
                || super::registry::has_class(id)
            {
                continue;
            }
            // A free ident outside those sets must be a capturable main-scope local.
            if !main_locals.contains(id) || body_assigns.contains(id) || main_mutated.contains(id) {
                sound = false; // unknown name / mutable / aliased — leave to bail.
                break;
            }
            captures.push(id.clone());
        }
        if !sound || captures.is_empty() {
            continue;
        }
        captures.sort();

        // Prepend each capture as a leading Tagged param; record the list.
        let mut new_params: Vec<HirParam> = captures
            .iter()
            .map(|n| HirParam {
                name: n.clone(),
                ty: HirType::Any,
                variadic: false,
                has_default: false,
                optional: false,
                default_expr: None,
            })
            .collect();
        new_params.extend(f.params.iter().cloned());
        f.params = new_params;
        ctx.captures.insert(f.name.clone(), captures);
    }
}

/// The `let`/`const` names declared directly in `stmts` (one level — the names a
/// hoisted function at this scope could capture). Does not descend into nested
/// blocks/control flow (a capture across those is not in the accepted subset).
fn declared_locals(stmts: &[HirStmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_declared(stmts, &mut out);
    out
}

/// Recursively collect every `let`/`const` declared anywhere in the function —
/// including LOOP HEADERS and nested blocks. A block-scoped `let` a closure
/// captures+mutates is cellable exactly like a top-level one: its `let` stmt
/// allocates the cell EACH time it executes, so a per-iteration body `let`
/// naturally gets a fresh binding per pass (and the for-header `let` gets the
/// spec's per-iteration copy in the loop lowering).
fn collect_declared(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for s in stmts {
        match s {
            HirStmt::Let { name, .. } | HirStmt::Const { name, .. } => {
                out.insert(name.clone());
            }
            HirStmt::If { then, else_, .. } => {
                collect_declared(then, out);
                if let Some(e) = else_ {
                    collect_declared(e, out);
                }
            }
            HirStmt::Try {
                body,
                catch,
                finally,
            } => {
                collect_declared(body, out);
                if let Some(c) = catch {
                    collect_declared(&c.body, out);
                }
                if let Some(f) = finally {
                    collect_declared(f, out);
                }
            }
            HirStmt::Labeled { body, .. } => {
                collect_declared(std::slice::from_ref(body), out);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_declared(body, out);
            }
            HirStmt::For { init, body, .. } => {
                if let Some(init) = init {
                    collect_declared(std::slice::from_ref(init), out);
                }
                collect_declared(body, out);
            }
            HirStmt::ForOf { body, .. } | HirStmt::ForIn { body, .. } => {
                collect_declared(body, out);
            }
            HirStmt::Block(body) => collect_declared(body, out),
            _ => {}
        }
    }
}

struct Ctx {
    /// Names resolving to a top-level function (free refs to these are not captures).
    top_level: HashSet<String>,
    /// MODULE-LEVEL MUTABLE GLOBAL (#195) names: a free ref to one is NOT a capture
    /// — it resolves to the shared runtime CELL (`GCELL_GET/SET`), so a closure that
    /// reads/writes it sees the LIVE value, never a stale by-value snapshot. The
    /// `rts:test` harness's `__rtsCapturedOutput` (a top-level `let` the `print`
    /// helper writes, read inside every `expect(...)` callback) is the canonical case.
    module_globals: HashSet<String>,
    /// Functions synthesized from extracted arrows.
    synthesized: Vec<HirFunc>,
    /// synthesized closure fn name → ordered captured outer-local names.
    captures: HashMap<String, Vec<String>>,
    /// FUNCTION-LOCAL CELLS (#195): function name → its names that became cells.
    /// Populated by [`Self::try_extract`] when it accepts a captured-AND-mutated
    /// local as a cell instead of bailing — keyed under BOTH the declaring function
    /// ([`Self::current_fn`]) and the synthesized closure.
    cells: HashMap<String, HashSet<String>>,
    /// The function whose body is currently being rewritten (the declaring scope of
    /// any cell a closure here captures). Set before each `rewrite_block` for a fn.
    current_fn: String,
    /// The `let`/`const` names declared at the TOP LEVEL of [`Self::current_fn`]'s
    /// body — the only locals a captured-mutated name may be a cell of this
    /// increment (a deeper-block or param capture bails, kept sound).
    current_fn_lets: HashSet<String>,
    /// Names in the CURRENT function declared with a fn-valued initializer
    /// (`const factor = (..) => …`) — a capture of one is a CELL (mutual
    /// forward refs read the live value; see `arrow_decl_names`).
    current_fn_arrow_decls: HashSet<String>,
    /// The `let`/`const` NAME whose initializer is being rewritten right now —
    /// a capture of it inside an arrow is a SELF-REFERENCE (`const f = () =>
    /// … f() …`) and must be a CELL (live read), never a by-value snapshot of
    /// the not-yet-assigned binding.
    self_binding: Option<String>,
    /// Fresh-name counter.
    counter: usize,
    /// Name NAMESPACE for lifted arrows (`__rtsn_arrow_{ns}{counter}`): `"p"` on
    /// the PRELUDE build, `""` on the user build. The two sides are lowered as
    /// separate programs (each counter starts at 0) then merged last-wins — a
    /// shared namespace made a prelude arrow and a user arrow collide on
    /// `__rtsn_arrow_0` (duplicate define / silent replacement).
    arrow_ns: String,
}

impl Ctx {
    fn rewrite_block(
        &mut self,
        stmts: &mut [HirStmt],
        scope: &HashSet<String>,
        mutated: &HashSet<String>,
    ) {
        // A statement list introduces locals (let/const); accumulate them so a
        // later arrow referencing an earlier local is correctly seen as a capture.
        let mut scope = scope.clone();
        for s in stmts.iter_mut() {
            self.rewrite_stmt(s, &mut scope, mutated);
        }
    }

    fn rewrite_stmt(
        &mut self,
        s: &mut HirStmt,
        scope: &mut HashSet<String>,
        mutated: &HashSet<String>,
    ) {
        match s {
            HirStmt::Expr(e) => self.rewrite_expr(e, scope, mutated),
            HirStmt::Return(Some(e)) => self.rewrite_expr(e, scope, mutated),
            HirStmt::Return(None) => {}
            HirStmt::Let { name, init, .. } => {
                // SELF-REFERENCE window: `const f = (..) => … f(..) …` — while
                // rewriting the initializer, `name` is in scope (JS: the binding
                // exists, TDZ aside) and a capture of it is routed to a CELL
                // (the closure must read the LIVE fn value written after reify,
                // not an undefined snapshot).
                scope.insert(name.clone());
                let prev = self.self_binding.replace(name.clone());
                if let Some(e) = init {
                    self.rewrite_expr(e, scope, mutated);
                }
                self.self_binding = prev;
            }
            HirStmt::Const { name, init, .. } => {
                scope.insert(name.clone());
                let prev = self.self_binding.replace(name.clone());
                self.rewrite_expr(init, scope, mutated);
                self.self_binding = prev;
            }
            HirStmt::If { cond, then, else_ } => {
                self.rewrite_expr(cond, scope, mutated);
                self.rewrite_block(then, scope, mutated);
                if let Some(e) = else_ {
                    self.rewrite_block(e, scope, mutated);
                }
            }
            HirStmt::While { cond, body } => {
                self.rewrite_expr(cond, scope, mutated);
                self.rewrite_block(body, scope, mutated);
            }
            HirStmt::Block(b) => {
                self.rewrite_block(b, scope, mutated);
                // The engine's local map is FUNCTION-FLAT (a block `let` stays
                // a live Cranelift local after the block), and the HIR wraps a
                // MULTI-DECLARATOR `const m = …, n = …` in a synthetic Block —
                // the declared names must stay in scope for later capture
                // analysis (`Array.from(…, () => n + 1)` after the decl).
                for s in b.iter() {
                    if let HirStmt::Let { name, .. } | HirStmt::Const { name, .. } = s {
                        scope.insert(name.clone());
                    }
                }
            }
            // try/catch/finally: arrows inside the bodies must lift like anywhere
            // else (`catch { return xs.map(x => x + 1) }`). The catch binding is a
            // block-scoped name.
            HirStmt::Try {
                body,
                catch,
                finally,
            } => {
                self.rewrite_block(body, scope, mutated);
                if let Some(c) = catch {
                    let mut inner = scope.clone();
                    if let Some(b) = &c.binding {
                        inner.insert(b.clone());
                    }
                    self.rewrite_block(&mut c.body, &inner, mutated);
                }
                if let Some(f) = finally {
                    self.rewrite_block(f, scope, mutated);
                }
            }
            HirStmt::DoWhile { cond, body } => {
                self.rewrite_expr(cond, scope, mutated);
                self.rewrite_block(body, scope, mutated);
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let mut inner = scope.clone();
                if let Some(i) = init {
                    self.rewrite_stmt(i, &mut inner, mutated);
                }
                if let Some(c) = cond {
                    self.rewrite_expr(c, &inner, mutated);
                }
                if let Some(u) = update {
                    self.rewrite_expr(u, &inner, mutated);
                }
                self.rewrite_block(body, &inner, mutated);
            }
            HirStmt::ForOf {
                binding,
                iterable,
                body,
                ..
            } => {
                self.rewrite_expr(iterable, scope, mutated);
                let mut inner = scope.clone();
                inner.insert(binding.clone());
                self.rewrite_block(body, &inner, mutated);
            }
            HirStmt::ForIn {
                binding,
                object,
                body,
            } => {
                self.rewrite_expr(object, scope, mutated);
                let mut inner = scope.clone();
                inner.insert(binding.clone());
                self.rewrite_block(body, &inner, mutated);
            }
            HirStmt::Throw(e) => self.rewrite_expr(e, scope, mutated),
            HirStmt::Switch {
                discriminant,
                cases,
            } => {
                self.rewrite_expr(discriminant, scope, mutated);
                for c in cases.iter_mut() {
                    if let Some(t) = &mut c.test {
                        self.rewrite_expr(t, scope, mutated);
                    }
                    self.rewrite_block(&mut c.body, scope, mutated);
                }
            }
            // Other statement kinds are outside the lowering subset; any arrow in
            // them will reach the lowering and bail. Leave untouched.
            _ => {}
        }
    }

    fn rewrite_expr(
        &mut self,
        e: &mut HirExpr,
        scope: &HashSet<String>,
        mutated: &HashSet<String>,
    ) {
        // Depth-first: rewrite children first, then this node.
        match &mut e.kind {
            HirExprKind::Bin { lhs, rhs, .. } => {
                self.rewrite_expr(lhs, scope, mutated);
                self.rewrite_expr(rhs, scope, mutated);
            }
            HirExprKind::Unary { operand, .. } => self.rewrite_expr(operand, scope, mutated),
            HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
                self.rewrite_expr(target, scope, mutated);
                self.rewrite_expr(value, scope, mutated);
            }
            HirExprKind::Call { callee, args } => {
                self.rewrite_expr(callee, scope, mutated);
                for a in args.iter_mut() {
                    self.rewrite_expr(a, scope, mutated);
                }
            }
            HirExprKind::MethodCall { object, args, .. } => {
                self.rewrite_expr(object, scope, mutated);
                for a in args.iter_mut() {
                    self.rewrite_expr(a, scope, mutated);
                }
            }
            HirExprKind::Member { object, .. } => self.rewrite_expr(object, scope, mutated),
            HirExprKind::Index { object, index } => {
                self.rewrite_expr(object, scope, mutated);
                self.rewrite_expr(index, scope, mutated);
            }
            HirExprKind::Ternary { cond, then, else_ } => {
                self.rewrite_expr(cond, scope, mutated);
                self.rewrite_expr(then, scope, mutated);
                self.rewrite_expr(else_, scope, mutated);
            }
            HirExprKind::Array(elems) => {
                for el in elems.iter_mut() {
                    self.rewrite_expr(el, scope, mutated);
                }
            }
            HirExprKind::Object(fields) => {
                for (_, v) in fields.iter_mut() {
                    self.rewrite_expr(v, scope, mutated);
                }
            }
            // `new C(args)` — descend into the CTOR ARGS so an inline arrow there
            // (`new Proxy(t, { get: (_t, _k) => 42 })`, a callback passed to a ctor)
            // is lifted, not left to bail. The class is a NAME, never an arrow.
            HirExprKind::New { args, .. } => {
                for a in args.iter_mut() {
                    self.rewrite_expr(a, scope, mutated);
                }
            }
            // `await <expr>` — descend so an arrow inside an awaited expression lifts.
            HirExprKind::Await(inner) => self.rewrite_expr(inner, scope, mutated),
            HirExprKind::Arrow { .. } => {
                // Try to extract this arrow into a top-level function or closure. On
                // success, replace the node with an Ident of the synthesized name.
                // `scope` is the OUTER scope (which names are captures), `mutated`
                // the names reassigned in the enclosing function (which captures are
                // unsound by-value).
                if let Some(name) = self.try_extract(e, scope, mutated) {
                    e.kind = HirExprKind::Ident(name);
                    e.ty = HirType::Function {
                        params: Vec::new(),
                        ret: Box::new(HirType::Any),
                    };
                }
                // On failure (unsound capture/unsupported) leave the Arrow → the
                // lowering bails explicitly.
            }
            _ => {}
        }
    }

    /// Try to turn `e` (an `Arrow`) into a synthesized top-level function (no
    /// captures) or CLOSURE (captures prepended as leading params). Returns its
    /// name on success, `None` when it cannot be soundly lifted this increment
    /// (an unsound capture, async/generator/`this`, a variadic/defaulted param) —
    /// the arrow is then left for the lowering to bail.
    fn try_extract(
        &mut self,
        e: &mut HirExpr,
        scope: &HashSet<String>,
        mutated: &HashSet<String>,
    ) -> Option<String> {
        let HirExprKind::Arrow {
            params,
            ret,
            body,
            self_name,
            is_async,
        } = &e.kind
        else {
            return None;
        };
        let is_async = *is_async;
        // The synthesized name is minted UP FRONT so a NAMED fn-expression's
        // self-references can be renamed to it before the free-ident analysis
        // (the self-name is body-only scope; renamed, it resolves as this very
        // top-level fn — recursion works). A bailed extraction just skips a
        // counter number.
        let name = format!("__rtsn_arrow_{}{}", self.arrow_ns, self.counter);
        self.counter += 1;
        let self_name = self_name.clone();
        // A VARIADIC (`...rest`) param is allowed only in LAST position (JS
        // grammar) — the synthesized fn keeps the flag, so the sig records
        // `rest_param` and the uniform thunk packs the overflow into it. A
        // DEFAULTED param is fine: the synthesized fn keeps `default_expr` and
        // the callee-side `fill_default_params` prologue (every lowered fn
        // gets it) replaces an `undefined` argument with the default.
        if params
            .iter()
            .enumerate()
            .any(|(i, p)| p.variadic && i + 1 != params.len())
        {
            return None;
        }
        let param_names: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();

        // Build the function body and gather its free identifiers + the names it
        // ASSIGNS to (mutable-capture detection).
        let mut body_stmts: Vec<HirStmt> = match body {
            HirArrowBody::Expr(inner) => vec![HirStmt::Return(Some((**inner).clone()))],
            HirArrowBody::Block(stmts) => stmts.clone(),
        };
        // An ES5-style FUNCTION EXPRESSION with an explicit `this` param
        // (`function (this: any) { … this.x … }` — a defineProperty descriptor
        // method): the body still carries swc's `Raw("This(..)")`. Rewrite it to
        // `Ident("this")`, which the declared `this` param binds — the uniform
        // thunk fills it from `a0`, so a receiver-passing invoker (`obj.m()`
        // prop-call, the `__get_` accessor dispatch) binds `this` correctly.
        if param_names.contains("this") {
            super::class::rewrite_this_block(&mut body_stmts);
        }
        // Bind a NAMED fn-expression's self-name: internal references become
        // references to the synthesized top-level name (registered below), so
        // they are neither free nor captures.
        if let Some(sn) = &self_name {
            if !param_names.contains(sn) {
                rename_ident_stmts(&mut body_stmts, sn, &name);
            }
        }
        let mut free = HashSet::new();
        let mut bound = param_names.clone();
        // Seed with the body's OWN declarations (lexical hoisting): a nested
        // `const step = (..) => … step(..) …` self-references FORWARD of the
        // walk's insertion point — without the seed, `step` surfaced as a free
        // ident outside the enclosing scope and the whole outer arrow bailed
        // (the __awaiter executor pattern). TDZ reads aside, a body-declared
        // name is never a capture of the OUTER scope.
        bound.extend(declared_locals(&body_stmts));
        for s in &body_stmts {
            collect_free_stmt(s, &mut bound, &mut free);
        }
        let body_assigns = arrow_assigned_names(&body_stmts);

        // Partition the free idents: a param / global / top-level fn is NOT a
        // capture; anything else must be a CAPTURABLE outer local to be sound.
        let mut captures: Vec<String> = Vec::new();
        // Captures that are MUTATED (by the closure or the enclosing fn) → CELLS:
        // a by-value snapshot would be stale, but capturing the cell HANDLE by value
        // shares the live box. Recorded after the synthesized name exists.
        let mut cell_caps: Vec<String> = Vec::new();
        for id in &free {
            if param_names.contains(id)
                || id == &name
                || GLOBALS.contains(&id.as_str())
                // A top-level CLOSURE (a synthesized fn WITH captures) is NOT
                // skippable: its direct name cannot be called from a scope that
                // lacks its captured locals — fall through to the capture path,
                // which snapshots its REIFIED fn VALUE (env embedded).
                || (self.top_level.contains(id) && !self.captures.contains_key(id))
                || self.module_globals.contains(id)
                // An in-scope OUTER LOCAL/PARAM SHADOWS a same-named Registry
                // namespace/class (JS scoping): `parse(input: string)` captures
                // the param even though an `input` namespace exists. Only a
                // NON-shadowed name resolves to the Registry surface.
                || (!scope.contains(id)
                    && (super::registry::has_namespace(id) || super::registry::has_class(id)))
            {
                continue;
            }
            // A top-level closure name: capture its fn VALUE (the enclosing
            // body reifies it — ITS captures are that body's leading params, so
            // the env embeds transitively). Never a mutable-capture cell.
            if self.top_level.contains(id) {
                captures.push(id.clone());
                continue;
            }
            // A free ident outside those sets is a CAPTURE. It must be a real outer
            // local in scope (not an unknown name / `this`) — OR a fn-valued
            // declaration of the enclosing body not yet reached by the rewrite
            // walk (a FORWARD reference between nested fns: `a` calls `b`
            // declared below); those are cell captures, resolved below.
            if !scope.contains(id) && !self.current_fn_arrow_decls.contains(id.as_str()) {
                return None; // unknown name / `this` — not a capturable local.
            }
            // A MUTATED capture (assigned by the closure body OR reassigned in the
            // enclosing function) is sound ONLY as a CELL (#195): the local must be a
            // TOP-LEVEL `let` of THIS function (so its `let` can allocate the cell and
            // the handle is captured by value). A mutated param / deeper-block local
            // is outside this increment → bail (sound floor, never a stale snapshot).
            // A capture of a local that is ALREADY a CELL in the enclosing fn
            // (its env snapshot carries the raw HANDLE, never the value) must
            // stay a cell in the new closure too — else the closure reads the
            // handle AS the value (`x === wrap` compared W to the box).
            let already_cell = self
                .cells
                .get(&self.current_fn)
                .is_some_and(|s| s.contains(id));
            if body_assigns.contains(id)
                || mutated.contains(id)
                || already_cell
                // A fn-valued declaration of the enclosing body: a FORWARD
                // mutual reference must read the LIVE value (cell), never a
                // reify-time snapshot (undefined before its decl runs).
                || self.current_fn_arrow_decls.contains(id.as_str())
                // SELF-REFERENCE (`const f = () => … f() …`): the binding is
                // written right after the reify, so a by-value snapshot would
                // capture undefined — a CELL reads the live fn value.
                || self.self_binding.as_deref() == Some(id.as_str())
            {
                if already_cell {
                    cell_caps.push(id.clone());
                    captures.push(id.clone());
                    continue;
                }
                if self.current_fn_lets.contains(id) {
                    cell_caps.push(id.clone());
                    captures.push(id.clone());
                    continue;
                }
                return None; // mutable param / non-let-local capture — bail.
            }
            captures.push(id.clone());
        }
        // Stable order: the env layout (and the leading param order) is the sorted
        // capture list, so reify and thunk agree deterministically.
        captures.sort();

        self.top_level.insert(name.clone());

        // Record each CELL capture under BOTH the declaring function (its `let`
        // allocates the cell) and the synthesized closure (its captured leading param
        // holds the handle) — both sides route reads/writes through the cell.
        for c in &cell_caps {
            self.cells
                .entry(self.current_fn.clone())
                .or_default()
                .insert(c.clone());
            self.cells
                .entry(name.clone())
                .or_default()
                .insert(c.clone());
        }

        // A captured lexical `this` (an arrow inside a method): the CALL-SITE
        // snapshot still reads the local named `this` (the method's param — the
        // captures MAP keeps the original name), but the SYNTH param + body must
        // use a rename: a leading param literally named `this` would mark the
        // closure `has_this`, and the reify path refuses `this`-using functions
        // as VALUES. Position in the sorted list is preserved (the env ABI is
        // positional). Capture-by-value is exact for `this` (never reassigned).
        let this_rename = "__rtsn_capthis";
        let mut body_stmts = body_stmts;
        if captures.iter().any(|c| c == "this") {
            rename_ident_stmts(&mut body_stmts, "this", this_rename);
        }

        // Prepend each captured var as a leading Tagged param (the thunk fills these
        // from the env array; the arrow's own params follow, filled from a0..a3).
        let mut all_params: Vec<HirParam> = captures
            .iter()
            .map(|n| HirParam {
                name: if n == "this" {
                    this_rename.to_string()
                } else {
                    n.clone()
                },
                ty: HirType::Any,
                variadic: false,
                has_default: false,
                optional: false,
                default_expr: None,
            })
            .collect();
        all_params.extend(params.iter().cloned());

        if !captures.is_empty() {
            self.captures.insert(name.clone(), captures);
        }
        // Um `async () =>` / `async function` aninhada lifta com `is_async` — o
        // CALL-SITE spawn (`lower_async_spawn`) faz a chamada devolver a Promise
        // pendente, o mesmo modelo da declaração top-level. Capturas mudam a
        // aridade real (params prepostos) — o spawn empacota args crus, então
        // capturas por valor viajam como qualquer arg.
        self.synthesized.push(HirFunc {
            name: name.clone(),
            params: all_params,
            ret: ret.clone(),
            body: body_stmts,
            is_async,
            is_arrow: true,
        });
        Some(name)
    }
}
