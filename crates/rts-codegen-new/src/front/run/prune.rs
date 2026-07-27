//! PRELUDE REACHABILITY PRUNING — drop embedded-stdlib functions the program
//! cannot reach, before Cranelift ever sees them.
//!
//! ## Why
//!
//! The engine's embedded `.ts` prelude (Error/Object/String/Number/console/Map/
//! Set/JSON/streams/`rts:test`/…) is ~250 KB of source that lowers to ~830
//! `HirFunc`s. Every one of them was declared, lowered and machine-compiled on
//! EVERY process start — measured 578 ms of Cranelift time for the function
//! bodies plus 89 ms for their uniform-ABI thunks, on a program that uses none
//! of them. Since `rts test` spawns one process per test file, that fixed cost
//! is paid hundreds of times per suite run.
//!
//! ## The rule (conservative by construction)
//!
//! A prelude function is KEPT when it is reachable from the roots; everything
//! else is dropped. USER functions and `__rts_startup` are ALWAYS roots and are
//! never pruned — this pass only ever removes prelude-origin functions.
//!
//! Reachability is computed over NAMES, not call edges, because dispatch is not
//! always static: a method can be reached through a shape/`__rts_class` runtime
//! table, an accessor through property syntax, a function through a value
//! reify. So from every reachable body we harvest EVERY name it mentions —
//! identifiers, called method names, member/property names, object-literal keys,
//! `new C` class names, and string literals (a dynamic `obj[name]` /
//! runtime-table lookup can only name a member it spells out somewhere) — and
//! treat each mention as a potential edge:
//!
//! * a mention matching a FUNCTION name keeps that function;
//! * a mention matching a CLASS name keeps that whole class (ctor, methods,
//!   accessors, statics, static fields) and, transitively, its ancestors — a
//!   subclass method can `super.m()` into the parent, and the runtime method
//!   table is registered per class;
//! * a mention matching a MEMBER name of any class keeps that member's function
//!   on EVERY class declaring it, because the receiver's class is not always
//!   statically known (`x.toString()` where `x: any`).
//!
//! The last rule is what makes the pass safe under dynamic dispatch, and it is
//! also why the pruning is not maximal — a program mentioning `log` keeps every
//! `log` method in the prelude. That is the intended trade: correctness first.
//!
//! ## A member name COMPUTED at runtime
//!
//! The name-mention premise would break for a name ASSEMBLED at runtime, since
//! nothing spells the result out:
//!
//! ```js
//! obj["to" + "String"]()   // neither "to" nor "String" matches a member
//! ```
//!
//! **For every receiver that is an OBJECT, the class invariant already covers
//! this.** An instance can only exist if some KEPT body named its class (`new C`,
//! an object literal's synthesized class, or a gcell singleton) — and naming a
//! class keeps that class's ENTIRE member surface, not merely the mentioned
//! members. Verified: `(o as any)["gre"+"et"]()` on an object literal and
//! `(c as any)["hel"+"lo"]()` on a user class both work with this pass enabled.
//!
//! **A PRIMITIVE receiver is covered by always keeping the intrinsic wrappers.**
//! A primitive has no recorded `[[Prototype]]`, so a property access on one
//! autoboxes: the value model maps the value's TAG to `String`/`Number`/
//! `Boolean`.prototype and walks that chain. Nothing in the program has to spell
//! the wrapper's name for that to happen, and a runtime-built member name spells
//! no member either — so neither edge kind can keep those classes. They are
//! therefore seeded as roots UNCONDITIONALLY (see `prune_prelude`). Measured
//! cost: 280 -> 293 kept functions.
//!
//! A general widening was tried and REJECTED on measurement: keeping the full
//! surface of every class touched by a computed access re-expanded the kept set
//! from 280 to 673 functions, because ordinary numeric `arr[i]` indexing trips
//! the same flag and the prelude is full of it. The unconditional three-wrapper
//! seed costs 13 functions and needs no flag — and a flag that is wrong in the
//! other direction is a miscompile, not a slowdown.
//!
//! `RTS_NO_PRUNE=1` disables the whole pass, confirming or clearing it as the
//! suspect for a misbehaving program in one run.

use std::collections::{HashMap, HashSet};

use rts_hir::ir::{HirArrowBody, HirExpr, HirExprKind, HirFunc, HirLit, HirStmt};

use super::LoweredProgram;
use super::class::ClassTable;

/// Prelude functions the LOWERING calls by a name the user source never
/// contains, so no mention edge can reach them. They are all members of the
/// PRIMORDIAL Object/String/Number/Boolean surface, which the engine may name.
///
/// This list must equal the set of prelude names spelled in the engine. Re-derive
/// it with:
///
/// ```text
/// grep -rhoE 'call_synth_fn\(module, "[A-Za-z_0-9]+"' crates/rts-codegen-new/src/
/// grep -rhoE 'sigs\.contains_key\("[A-Za-z_0-9]+"\)'   crates/rts-codegen-new/src/
/// ```
///
/// `prelude_synth_fns_exist` below fails if one of these disappears from the
/// prelude (a rename), which is the drift this list is exposed to.
const ENGINE_CALLED_PRELUDE_FNS: &[&str] = &[
    // `Object(x)` / `new Object(x)` → `ObjectFactory(x)` (globals.rs, newexpr.rs)
    "ObjectFactory",
    // `Number(x)` called as a conversion. `StringFactory` no longer exists
    // (String moved to a pure-Rust value-class); `BooleanFactory` likewise
    // (Boolean moved — see `rts-primitives/src/boolean.rs`).
    "NumberFactory",
    // `Object.groupBy(..)` → `__object_group_by(..)` (objstatic.rs)
    "__object_group_by",
];

/// Drop every prelude function unreachable from `main` + the user functions.
/// A no-op when there is no prelude, or when `RTS_NO_PRUNE` is set.
pub(super) fn prune_prelude(prog: &mut LoweredProgram) {
    // Skip for AOT: the prune only cuts JIT startup, buys a one-shot AOT compile
    // nothing, and 8× a pre-existing AOT startup crash (see
    // `module_entry::compile_path_to_object`).
    if prog.prelude_fns.is_empty() || disabled() || super::aot_str::aot_mode() {
        return;
    }
    let before = prog.funcs.len();

    // Sub-phase timing: `prune prelude` measures ~8 ms of the ~54 ms startup and
    // the suspicion is that `Index::build`'s two `to_string()` per edge dominate.
    // Split it so the next optimization is aimed by measurement rather than by
    // reading the code and guessing which half is hot.
    let index = crate::timing::phase("  prune: index build", || {
        Index::build(&prog.funcs, &prog.classes, &prog.fn_this_class)
    });
    let mut keep: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = Vec::new();

    // ROOTS. Every non-prelude (user) function, plus every name `main` mentions.
    // `main` itself is not in `funcs` and is never pruned.
    for f in &prog.funcs {
        if !prog.prelude_fns.contains(&f.name) {
            keep.insert(f.name.clone());
            queue.push(f.name.clone());
        }
    }
    let mut mentions: HashSet<String> = HashSet::new();
    harvest_func(&prog.main, &mut mentions);
    // A gcell-promoted singleton (`const console = new Console()`) records its
    // class here; the `new` in main already mentions it, but the map is also the
    // lowerer's recovery path for a receiver whose `new` was erased — mention
    // both sides so the class survives either way.
    for (name, class) in &prog.gcell_classes {
        mentions.insert(name.clone());
        mentions.insert(class.clone());
    }
    // A captured/celled name is read by a closure that may itself be prelude
    // code; the capture lists spell the names out.
    for names in prog.captures.values() {
        mentions.extend(names.iter().cloned());
    }
    // The INTRINSIC WRAPPERS are always kept WHOLE. A primitive receiver has no
    // recorded `[[Prototype]]`, so a property access on one autoboxes: the value
    // model maps the VALUE'S TAG to `String`/`Number`/`Boolean`.prototype
    // (`rts-adapters` `protos::intrinsic_proto`) and walks that. Nothing in the
    // program has to spell the wrapper's name for that path to be taken, and a
    // RUNTIME-built member name (`n["to" + "String"]()`) spells no member either
    // — so member-name edges cannot keep these, and pruning them breaks a call
    // that genuinely resolves. Measured: without this, the agent-authored
    // `computed_member_primitive` fixture fails with `toString is not a
    // function`.
    //
    // These three are PRIMORDIAL, so the engine may name them (see the
    // PRIMORDIAL-vs-Registry doctrine in `CLAUDE.md`). Keeping them
    // UNCONDITIONALLY rather than behind a "does this program do a computed
    // access?" flag is deliberate: the flag is trippable by ordinary numeric
    // `arr[i]` indexing, which made an earlier attempt re-expand the kept set
    // from 280 to 673 functions, and a flag that is wrong in the OTHER direction
    // is a miscompile. The cost here is bounded and measured — three classes.
    for wrapper in ["String", "Number", "Boolean"] {
        mentions.insert(wrapper.to_string());
    }
    // …and so are the prelude helpers the ENGINE ITSELF calls by name. The
    // lowering rewrites certain syntax to a prelude function the source never
    // spells — `Object(x)` / `new Object(x)` become `ObjectFactory(x)`
    // (`globals.rs`, `newexpr.rs`), `Object.groupBy` becomes
    // `__object_group_by` (`objstatic.rs`). A name-reachability pass cannot see
    // a rewrite that happens AFTER it, so these must be seeded.
    //
    // Missing this was a real regression: `console.log(typeof Object(null))`
    // failed with "call to unknown function `Object`" once the factory was
    // pruned. It went unnoticed because the crate's unit tests could not run to
    // completion at the time — see `ENGINE_CALLED_PRELUDE_FNS`.
    for name in ENGINE_CALLED_PRELUDE_FNS {
        mentions.insert((*name).to_string());
    }

    // FIXPOINT. Alternate the two edge kinds until neither adds anything: harvest
    // the bodies newly added to the queue, then expand every fresh mention into
    // the functions it can reach (which refills the queue). Both `keep` and
    // `seen_mentions` only ever grow, so this terminates.
    let mut seen_mentions: HashSet<String> = HashSet::new();
    let fixpoint_start = std::time::Instant::now();
    loop {
        // (a) harvest the bodies of everything queued.
        for name in std::mem::take(&mut queue) {
            if let Some(f) = index.func(&name) {
                harvest_func(f, &mut mentions);
            }
        }
        // (b) expand the mentions not yet expanded.
        let fresh: Vec<String> = std::mem::take(&mut mentions)
            .into_iter()
            .filter(|m| seen_mentions.insert(m.clone()))
            .collect();
        if fresh.is_empty() {
            break;
        }
        for m in fresh {
            for name in index.functions_for(&m) {
                if keep.insert(name.clone()) {
                    queue.push(name);
                }
            }
        }
    }

    crate::timing::report("  prune: fixpoint", fixpoint_start);

    // APPLY. Drop unreachable prelude functions and every per-function side map
    // keyed by their names (a stale entry would keep a dropped fn's thunk or
    // capture list alive downstream).
    prog.funcs
        .retain(|f| keep.contains(&f.name) || !prog.prelude_fns.contains(&f.name));
    let live: HashSet<&String> = prog.funcs.iter().map(|f| &f.name).collect();
    prog.fn_this_class.retain(|k, _| live.contains(k));
    prog.captures.retain(|k, _| live.contains(k));
    prog.cells.retain(|k, _| live.contains(k));
    prog.param_classes.retain(|k, _| live.contains(k));
    prog.prelude_fns.retain(|k| live.contains(k));

    crate::timing::note("prelude fns pruned", before - prog.funcs.len());
    crate::timing::note("funcs kept", prog.funcs.len());
}

/// For every class with at least one KEPT function, keep its whole member
/// surface (and its ancestors'). Returns whether anything new was added, so the
/// caller's fixpoint knows to harvest the newly kept bodies.
///
/// This is the computed-member-access widening: a class kept only through
/// member-name edges — the primitive method libs, reached as `"abc".toUpperCase()`
/// without ever naming `String` — would otherwise hold only the members the
/// program spells out, and a computed subscript can ask for any of them.

/// `RTS_NO_PRUNE` escape hatch (any non-empty, non-`0` value disables).
fn disabled() -> bool {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OFF.get_or_init(|| match std::env::var("RTS_NO_PRUNE") {
        Ok(v) => !v.is_empty() && v != "0",
        Err(_) => false,
    })
}

/// Name → the functions a mention of it can reach.
struct Index<'a> {
    by_name: HashMap<&'a str, &'a HirFunc>,
    /// A mentioned name → every function name it can pull in (itself as a
    /// function, a whole class, or a member across all classes declaring it).
    /// Borrowed, not owned: see the note in [`Index::build`] — the owned form
    /// allocated two `String`s per edge and dominated the whole prune pass.
    edges: HashMap<&'a str, Vec<&'a str>>,
}

impl<'a> Index<'a> {
    fn build(
        funcs: &'a [HirFunc],
        classes: &'a ClassTable,
        fn_this_class: &'a HashMap<String, String>,
    ) -> Self {
        let by_name: HashMap<&str, &HirFunc> = funcs.iter().map(|f| (f.name.as_str(), f)).collect();
        // Edges BORROW their names from `funcs`/`classes`, which outlive the
        // index. The owned version allocated two `String`s per edge, and an edge
        // exists for every function, every class member, and every
        // class-x-ancestor pair — measured at 7.07 ms of the ~10 ms prune (the
        // fixpoint that follows is 0.79 ms), i.e. building the index cost 9x what
        // using it did. Borrowing removes the allocation entirely.
        let mut edges: HashMap<&'a str, Vec<&'a str>> = HashMap::new();
        let mut add = |key: &'a str, val: &'a str| {
            edges.entry(key).or_default().push(val);
        };

        // A function name reaches itself.
        for f in funcs {
            add(&f.name, &f.name);
        }
        for desc in classes.iter() {
            // A CLASS name reaches its whole synthesized surface — and that of
            // every ANCESTOR, because a kept subclass method can `super.m()`
            // into any of them and the runtime method table is per class.
            let mut cur = Some(desc);
            while let Some(c) = cur {
                for fname in class_fns(c) {
                    add(desc.name.as_str(), fname);
                }
                cur = c.parent.as_deref().and_then(|p| classes.get(p));
            }
            // A MEMBER name reaches that member's fn on THIS class — the
            // receiver's class is not always statically known, so a mention of
            // `toString` must keep every `toString` in the program.
            for (member, fname) in members_of(desc) {
                add(member, fname);
            }
        }
        // A synthesized method fn name reaches its OWNING class: the body can
        // `this.sibling()` / `super.m()` through the class table rather than a
        // spelled-out function name.
        for (fname, class) in fn_this_class {
            if let Some(desc) = classes.get(class) {
                let mut cur = Some(desc);
                while let Some(c) = cur {
                    for f in class_fns(c) {
                        add(fname.as_str(), f);
                    }
                    cur = c.parent.as_deref().and_then(|p| classes.get(p));
                }
            }
        }
        Self { by_name, edges }
    }

    fn func(&self, name: &str) -> Option<&'a HirFunc> {
        self.by_name.get(name).copied()
    }

    /// The REAL functions a mention of `mention` can reach (class/member edges
    /// were already expanded to function names at build time).
    fn functions_for(&self, mention: &str) -> Vec<String> {
        let Some(names) = self.edges.get(mention) else {
            return Vec::new();
        };
        names
            .iter()
            .filter(|n| self.by_name.contains_key(*n))
            .map(|n| (*n).to_string())
            .collect()
    }
}

/// Every synthesized function name a class owns (ctor, methods, accessors,
/// statics, static-field getters).
/// Borrows from `desc` rather than cloning: the caller holds the `ClassTable`
/// for the whole index build, and this runs once per class PER ANCESTOR, so the
/// clones were quadratic in inheritance depth for no benefit.
fn class_fns(desc: &super::class::ClassDesc) -> Vec<&str> {
    let mut out = vec![desc.ctor.as_str()];
    out.extend(desc.methods.values().map(String::as_str));
    out.extend(desc.statics.values().map(String::as_str));
    out.extend(desc.static_fields.values().map(String::as_str));
    for acc in desc.accessors.values() {
        if let Some(g) = &acc.getter {
            out.push(g.as_str());
        }
        if let Some(s) = &acc.setter {
            out.push(s.as_str());
        }
    }
    out
}

/// Member NAME → its synthesized function name, for every dispatchable member.
fn members_of(desc: &super::class::ClassDesc) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    for (m, f) in &desc.methods {
        out.push((m.as_str(), f.as_str()));
    }
    for (m, f) in &desc.statics {
        out.push((m.as_str(), f.as_str()));
    }
    for (m, f) in &desc.static_fields {
        out.push((m.as_str(), f.as_str()));
    }
    for (m, acc) in &desc.accessors {
        if let Some(g) = &acc.getter {
            out.push((m.as_str(), g.as_str()));
        }
        if let Some(s) = &acc.setter {
            out.push((m.as_str(), s.as_str()));
        }
    }
    out
}

/// Harvest every NAME mentioned anywhere in `f`'s body into `out`.
fn harvest_func(f: &HirFunc, out: &mut HashSet<String>) {
    for s in &f.body {
        harvest_stmt(s, out);
    }
}

/// EXHAUSTIVE statement walk. Written out variant by variant (no `_ =>` arm) on
/// purpose: a name this walk misses is a function that gets pruned while still
/// being called, so a new HIR statement must break this match at compile time
/// rather than silently produce a broken program.
fn harvest_stmt(s: &HirStmt, out: &mut HashSet<String>) {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => harvest_expr(e, out),
        HirStmt::Return(e) => {
            if let Some(e) = e {
                harvest_expr(e, out);
            }
        }
        HirStmt::Let { name, init, .. } => {
            out.insert(name.clone());
            if let Some(e) = init {
                harvest_expr(e, out);
            }
        }
        HirStmt::Const { name, init, .. } => {
            out.insert(name.clone());
            harvest_expr(init, out);
        }
        HirStmt::If { cond, then, else_ } => {
            harvest_expr(cond, out);
            harvest_block(then, out);
            if let Some(b) = else_ {
                harvest_block(b, out);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            harvest_expr(cond, out);
            harvest_block(body, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                harvest_stmt(i, out);
            }
            if let Some(c) = cond {
                harvest_expr(c, out);
            }
            if let Some(u) = update {
                harvest_expr(u, out);
            }
            harvest_block(body, out);
        }
        HirStmt::ForOf {
            binding,
            iterable,
            body,
            ..
        } => {
            out.insert(binding.clone());
            harvest_expr(iterable, out);
            harvest_block(body, out);
        }
        HirStmt::ForIn {
            binding,
            object,
            body,
            ..
        } => {
            out.insert(binding.clone());
            harvest_expr(object, out);
            harvest_block(body, out);
        }
        HirStmt::Break(_) | HirStmt::Continue(_) => {}
        HirStmt::Raw(n) => {
            out.insert(n.clone());
        }
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            harvest_block(body, out);
            if let Some(c) = catch {
                if let Some(b) = &c.binding {
                    out.insert(b.clone());
                }
                harvest_block(&c.body, out);
            }
            if let Some(f) = finally {
                harvest_block(f, out);
            }
        }
        HirStmt::Switch {
            discriminant,
            cases,
        } => {
            harvest_expr(discriminant, out);
            for c in cases {
                if let Some(t) = &c.test {
                    harvest_expr(t, out);
                }
                harvest_block(&c.body, out);
            }
        }
        HirStmt::Block(b) => harvest_block(b, out),
        HirStmt::Labeled { body, .. } => harvest_stmt(body, out),
    }
}

fn harvest_block(b: &[HirStmt], out: &mut HashSet<String>) {
    for s in b {
        harvest_stmt(s, out);
    }
}

/// EXHAUSTIVE expression walk — same no-`_`-arm discipline as [`harvest_stmt`].
fn harvest_expr(e: &HirExpr, out: &mut HashSet<String>) {
    match &e.kind {
        // A dynamic `obj[name]` / runtime method-table lookup can only reach a
        // member the program spells out somewhere; a string literal is that
        // spelling, so every one of them counts as a potential member mention.
        HirExprKind::Lit(HirLit::Str(s)) => {
            out.insert(s.clone());
        }
        HirExprKind::Lit(_) => {}
        HirExprKind::Ident(n) | HirExprKind::Raw(n) => {
            out.insert(n.clone());
        }
        HirExprKind::Bin { lhs, rhs, .. } => {
            harvest_expr(lhs, out);
            harvest_expr(rhs, out);
        }
        HirExprKind::Unary { operand, .. } => harvest_expr(operand, out),
        HirExprKind::Assign { target, value } => {
            harvest_expr(target, out);
            harvest_expr(value, out);
        }
        HirExprKind::AssignOp { target, value, .. } => {
            harvest_expr(target, out);
            harvest_expr(value, out);
        }
        HirExprKind::Call { callee, args } => {
            harvest_expr(callee, out);
            harvest_all(args, out);
        }
        HirExprKind::MethodCall {
            object,
            method,
            args,
        } => {
            out.insert(method.clone());
            harvest_expr(object, out);
            harvest_all(args, out);
        }
        HirExprKind::New { class, args } => {
            out.insert(class.clone());
            harvest_all(args, out);
        }
        HirExprKind::Member { object, prop } => {
            out.insert(prop.clone());
            harvest_expr(object, out);
        }
        HirExprKind::Index { object, index } => {
            harvest_expr(object, out);
            harvest_expr(index, out);
        }
        HirExprKind::Array(elems) => harvest_all(elems, out),
        HirExprKind::Object(entries) => {
            for (k, v) in entries {
                out.insert(k.clone());
                harvest_expr(v, out);
            }
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            harvest_expr(cond, out);
            harvest_expr(then, out);
            harvest_expr(else_, out);
        }
        HirExprKind::Await(inner)
        | HirExprKind::Spread(inner)
        | HirExprKind::PreInc(inner)
        | HirExprKind::PreDec(inner)
        | HirExprKind::PostInc(inner)
        | HirExprKind::PostDec(inner)
        | HirExprKind::Cast { expr: inner, .. } => harvest_expr(inner, out),
        HirExprKind::Seq(exprs) => harvest_all(exprs, out),
        // An arrow surviving into a body (not lifted) still names things.
        HirExprKind::Arrow {
            params,
            body,
            self_name,
            ..
        } => {
            if let Some(n) = self_name {
                out.insert(n.clone());
            }
            for p in params {
                out.insert(p.name.clone());
            }
            match body {
                HirArrowBody::Expr(inner) => harvest_expr(inner, out),
                HirArrowBody::Block(stmts) => harvest_block(stmts, out),
            }
        }
    }
}

fn harvest_all(es: &[HirExpr], out: &mut HashSet<String>) {
    for e in es {
        harvest_expr(e, out);
    }
}

#[cfg(test)]
mod tests {
    use super::ENGINE_CALLED_PRELUDE_FNS;

    /// Every name in [`ENGINE_CALLED_PRELUDE_FNS`] must still be DEFINED by the
    /// embedded prelude. The list exists because the lowering calls these by a
    /// name no user source spells, so nothing else would keep them alive — and a
    /// rename in the `.ts` would otherwise silently turn the seed into a no-op
    /// and re-introduce "call to unknown function `Object`".
    #[test]
    fn prelude_synth_fns_exist() {
        let prelude = crate::front::run::registry::includes_prelude();
        for name in ENGINE_CALLED_PRELUDE_FNS {
            assert!(
                prelude.contains(&format!("function {name}(")),
                "`{name}` is seeded as a prune root but the prelude no longer \
                 defines it — re-derive the list (see its doc comment)"
            );
        }
    }
}
