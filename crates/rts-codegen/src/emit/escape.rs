//! Which object literals never have to become objects.
//!
//! # What this buys, measured before it was written
//!
//! `let o = {a: i}; t = t + o.a;` emits **three** runtime calls: `ObjectNew`, a
//! property write and a property read. A call site costs about **100
//! microseconds of Cranelift code generation** each, so the three are 300 of
//! them at compile time, and at run time the object is a heap allocation with
//! two cache sites hanging off it.
//!
//! The allocation is worse than slow. A region holds 65 536 cells and nothing
//! reclaims them, so a loop allocating past that **ends the program**. An
//! allocation removed here is not an optimisation of a program that ran; it is a
//! program that runs at all.
//!
//! # What is replaced
//!
//! An object literal assigned to a local, where the local provably does not
//! escape and every access to it names a key the compiler knows. The object
//! becomes one ordinary binding per property, and the whole thing — the
//! allocation, the writes, the reads — becomes values in registers:
//!
//! ```text
//! let o = {a: 1, b: 2};        →   let «o.a» = 1; let «o.b» = 2;
//! o.a = o.a + o.b;             →   «o.a» = «o.a» + «o.b»;
//! return o.a;                  →   return «o.a»;
//! ```
//!
//! Nothing new is invented to hold the properties: `«o.a»` is a name declared
//! through [`super::binding`] like any other local, so it gets the SSA merging
//! at an `if`, the shadowing, and the representation rules for free. Inventing a
//! second kind of local would have meant restating all of that, and the merge
//! rules are the part where a second statement of them goes wrong silently.
//!
//! # Why this is not a fixpoint, where [`super::proven`] is one
//!
//! `proven` starts optimistic and shrinks until stable, because "numeric" for
//! one local *depends on* another being numeric — `let a = 1; let b = a;` needs
//! a second round. Here the dependency runs the other way: a literal may hold a
//! read of a local, and `let p = {x: q}` is replaceable **only if `q` is not**,
//! which the escape scan settles by itself — `q` appearing bare in an expression
//! is exactly what kills `q`. Removing candidates can therefore never create
//! one, so one collection pass and one escape pass is the whole answer, and a
//! loop around them would be ceremony that converges on the first round.
//!
//! # The test is an allow-list, and the default is "escapes"
//!
//! Two positions keep a candidate alive: a read `o.k` and a write `o.k = v`,
//! both with `k` among the literal's own keys. **Every other position a name can
//! appear in kills it**, including ones this file has never considered — the
//! general path kills any bare identifier it reaches, and a node whose children
//! the shared walker does not descend into kills every candidate outright.
//!
//! That direction is chosen rather than convenient. A local wrongly kept is a
//! program that still allocates; a local wrongly replaced is an object that a
//! caller, a closure or a property read can still reach and that no longer
//! exists — a wrong program that runs.
//!
//! # Why the traversal is [`super::capture`]'s and not a new one
//!
//! Because a second description of the tree's shape is how a node comes to be
//! walked by one analysis and skipped by another, and this codebase has been
//! bitten by exactly that twice in a week. `walk_expr` and `walk_stmt` are the
//! one description; what this file adds is which *position* a child sits in,
//! which the walker deliberately does not say.
//!
//! Three places where the shared walker does not descend are handled by killing
//! every candidate rather than by writing the missing traversal: a template, a
//! tagged template, and `super`. Its comment says the emitter refuses those, and
//! for the template that is **no longer true** — `emit/template.rs` emits one.
//! Fixing the walker is a change to `capture.rs`, which is where it belongs and
//! is not this file's to make; what this file owes is not to depend on the
//! stale claim.
//!
//! # The ordering rule
//!
//! `{a: f(), b: g()}` evaluates `f` then `g`, once each. Replacing the literal
//! has to keep that, and it does: the property values are emitted in source
//! order at the declaration, so the same expressions run the same number of
//! times in the same order.
//!
//! A property value used to have to be a literal or a bare name on top of that.
//! It no longer does — the argument is written out at [`flattenable`], with the
//! measurement of what the restriction cost. What replaced it is narrower and
//! actually load-bearing: a read of the name **in its own initialiser** is a
//! `ReferenceError`, not an access, and [`Escaping::declaring`] is what says so.
//! A duplicate key is still refused, which is what keeps "the same number of
//! times" true.
//!
//! # The region rule
//!
//! A loop, a `try`, a `switch` and the rest carry values across their edges by
//! **name**: `loops::assigned_in_stmt` collects the names a body writes and
//! turns them into block parameters, and `capture::assigned_under_protection`
//! does the same for a handler. Neither can learn about `«o.a»`, because the
//! program does not spell it — `o.a = 1` records nothing at all today, correctly,
//! since it writes a heap property.
//!
//! So a write is refused when it sits deeper inside such a region than the
//! declaration does. A *read* is unrestricted: a value defined before a region
//! dominates every block in it, and reading one creates no second definition for
//! anything to merge.

use std::collections::{BTreeSet, HashMap, HashSet};

use rts_cranelift::ir::FuncBuilder;

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use super::{Ctx, EmitResult, Scope};
use crate::names::Name;
use crate::syntax::{
    AssignOp, AssignTarget, Expr, ExprKind, ForEachTarget, Pattern, Property, PropertyKey,
    Spreadable, Stmt, StmtKind, UnaryOp,
};

/// What a replaced property's binding is spelled with.
///
/// A dot is enough on its own — no identifier can hold one — but a string
/// literal used as a key can, and `ctx.names` interns those too. The prefix is
/// what makes a collision with `p["o.a"]` impossible rather than merely
/// unlikely, and it follows the spelling `binding::OUTER` already uses for a
/// name the compiler invents.
const MARKER: &str = "__rts_flat.";

/// The locals a function body holds an object in and never lets escape.
#[derive(Default, Debug)]
pub(super) struct Flattened {
    /// Each replaceable local, with its literal's keys in source order.
    objects: HashMap<Name, Vec<Name>>,
}

impl Flattened {
    /// The keys a replaced local's literal wrote, or `None` if it is not one.
    pub(super) fn properties(&self, object: Name) -> Option<&[Name]> {
        self.objects.get(&object).map(Vec::as_slice)
    }

    /// Whether an access names a replaced local and one of its own keys.
    pub(super) fn has(&self, object: Name, property: Name) -> bool {
        self.objects
            .get(&object)
            .is_some_and(|keys| keys.contains(&property))
    }
}

/// One local that might be replaceable, before the escape scan has run.
struct Candidate {
    /// Its literal's keys, in source order.
    keys: Vec<Name>,
    /// How many value-carrying regions enclose its declaration.
    depth: u32,
}

/// Decides which of a function body's locals hold an object nothing can reach.
///
/// `captured` is [`super::capture::captured`]'s answer, taken rather than
/// recomputed: which names nested code can see is decided once, and a second
/// answer to it would be a second chance to say "not captured" about a local a
/// closure holds.
pub(super) fn analyse(
    body: &[Stmt],
    parameters: &[Name],
    captured: &BTreeSet<Name>,
) -> Flattened {
    let mut collected = Collected::default();
    for statement in body {
        collect(statement, 0, &mut collected);
    }

    let mut escaping = Escaping {
        candidates: &collected.candidates,
        escaped: HashSet::new(),
        everything: false,
        declaring: None,
    };
    for statement in body {
        scan_stmt(statement, 0, &mut escaping);
    }
    if escaping.everything {
        return Flattened::default();
    }

    let objects = collected
        .candidates
        .iter()
        .filter(|(name, _)| !escaping.escaped.contains(name))
        // A captured local lives in an environment object, which is storage two
        // activations share. There is nothing to replace it with.
        .filter(|(name, _)| !captured.contains(name))
        // A parameter holds whatever a caller passed, and the declaration that
        // made this a candidate would be shadowing it.
        .filter(|(name, _)| !parameters.contains(name))
        // Exactly one binding in the whole body introduces this spelling. Two —
        // a redeclaration, a `catch` parameter, a loop target, an inner block's
        // own `let` — means the name refers to more than one thing, and this
        // analysis works in names rather than in bindings.
        .filter(|(name, _)| collected.introductions.get(name) == Some(&1))
        .map(|(name, candidate)| (*name, candidate.keys.clone()))
        .collect();
    Flattened { objects }
}

/// What the first pass finds.
#[derive(Default)]
struct Collected {
    /// Locals declared as a replaceable-looking literal.
    candidates: HashMap<Name, Candidate>,
    /// How many bindings introduce each spelling, in any form.
    introductions: HashMap<Name, usize>,
}

impl Collected {
    /// Records that something introduced a name.
    fn introduce(&mut self, name: Name) {
        *self.introductions.entry(name).or_default() += 1;
    }

    /// The same, for whatever a pattern binds.
    fn introduce_pattern(&mut self, pattern: &Pattern) {
        let mut bound = Vec::new();
        pattern.bound_names(&mut bound);
        for name in bound {
            self.introduce(name);
        }
    }
}

/// Whether a statement carries values across its own edges by name.
///
/// The list is what `loops.rs`, `protect.rs`, `switch.rs` and `foreach.rs`
/// build merges for. `Block` and `If` are absent on purpose and that is the
/// interesting half: an `if` merges by comparing two scope **snapshots**, which
/// names nothing and therefore sees a replaced property exactly as it sees any
/// other local. Only the constructs that collect names from the *tree* are
/// barriers, because the tree does not spell a replaced property.
fn is_barrier(statement: &Stmt) -> bool {
    matches!(
        statement.kind,
        StmtKind::While { .. }
            | StmtKind::DoWhile { .. }
            | StmtKind::For { .. }
            | StmtKind::ForEach { .. }
            | StmtKind::Try { .. }
            | StmtKind::Using { .. }
            | StmtKind::With { .. }
            | StmtKind::Switch { .. }
            | StmtKind::Labelled { .. }
    )
}

/// Finds candidates, and counts every binding that introduces a name.
fn collect(statement: &Stmt, depth: u32, into: &mut Collected) {
    if let StmtKind::Declare { kind, bindings } = &statement.kind {
        for binding in bindings {
            // Block-scoped only. A `var` is function-scoped and visible outside
            // the block it is written in, where the bindings that replace it
            // would have been popped with the block.
            if !kind.is_block_scoped() {
                continue;
            }
            let Pattern::Name(name) = &binding.target else {
                continue;
            };
            let Some(value) = &binding.value else {
                continue;
            };
            let ExprKind::Object { properties } = &value.kind else {
                continue;
            };
            let Some(keys) = flattenable(properties) else {
                continue;
            };
            into.candidates.insert(*name, Candidate { keys, depth });
        }
    }

    // A `for`-each target is not a sub-statement, a sub-expression, a binding or
    // a `catch`, so the shared walker is silent about it — its own doc says so.
    if let StmtKind::ForEach { target, .. } = &statement.kind {
        match target {
            ForEachTarget::Declare { target, .. } => into.introduce_pattern(target),
            ForEachTarget::Dispose { target, .. } => into.introduce(*target),
            // An assignment target introduces nothing; the escape scan is what
            // kills a candidate a loop writes over.
            ForEachTarget::Assign(_) => {}
        }
    }

    let inner = depth + u32::from(is_barrier(statement));
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(nested) => collect(nested, inner, into),
        StmtChild::Binding(binding) => into.introduce_pattern(&binding.target),
        StmtChild::Catch(catch) => {
            if let Some(pattern) = &catch.binding {
                into.introduce_pattern(pattern);
            }
            for nested in &catch.body {
                collect(nested, inner, into);
            }
        }
        // A declaration binds its own name in this scope, which is what makes a
        // `let o = {…}` beside a `function o() {}` two things under one name.
        StmtChild::Function(function) => {
            if let Some(name) = function.name {
                into.introduce(name);
            }
        }
        StmtChild::Class(class) => {
            if let Some(name) = class.name {
                into.introduce(name);
            }
        }
        // The first pass asks nothing of expressions: the object literal it
        // cares about was read above, and every other question is the second
        // pass's.
        StmtChild::Expr(_) => {}
    });
}

/// The keys a literal can be replaced by, or `None` if it cannot be.
///
/// Refused, each for its own reason: a **computed key** is not known here at
/// all; a **spread** copies however many properties another object has, which is
/// a count decided at run time; a **getter or setter** is code, and a method is
/// a closure with a home object; **`__proto__`** does not add a property but
/// changes what the object *is*. A **duplicate key** is refused because the
/// second write would land in a second binding and the first value would be
/// readable again, which is not what the object does.
///
/// Shorthand — `{a}` — is accepted, and that is not a guess: it is `{a: a}` with
/// the value recorded as an ordinary identifier, so nothing about it is special.
///
/// # Why the VALUES are not restricted
///
/// They were, to a literal or a bare name, and the restriction is gone. It was
/// costing the most ordinary literal there is — `{a: i, b: i + 1}` measured
/// 119.38 ms/compile against 42.85 ms for the same program without the `+ 1`
/// (2026-08-06, release, 400 statements) — and the three things it was
/// protecting are each answered somewhere better:
///
/// - **Evaluation order.** `declare_flattened` emits the values in source
///   order at the point the literal was written. Same expressions, same order,
///   same number of times. There is nothing left for a restriction to protect.
///
/// - **A value that mentions another candidate.** `{x: q}` puts `q` in the
///   tree as a bare name, and the escape scan kills any candidate it reaches
///   as one. The dependency runs the right way round, which is why this pass
///   needs no fixpoint.
///
/// - **A value that reads the object being declared.** `{a: 1, b: o.a}` is a
///   `ReferenceError` and must stay one. That is `Escaping::declaring`, and it
///   is a real hole this restriction was hiding rather than one it was solving.
///
/// What is left is a value that **throws** halfway, leaving some bindings
/// declared and the rest not. Nothing can observe it: the object provably does
/// not escape, `o` is block-scoped, and the exception leaves the block that
/// declares it. The half-built object in the unreplaced program is exactly as
/// unreachable.
fn flattenable(properties: &[Property]) -> Option<Vec<Name>> {
    let mut keys: Vec<Name> = Vec::new();
    for property in properties {
        let Property::Value {
            key: PropertyKey::Named(name),
            ..
        } = property
        else {
            return None;
        };
        if keys.contains(name) {
            return None;
        }
        keys.push(*name);
    }
    Some(keys)
}

/// What the escape scan is accumulating.
struct Escaping<'a> {
    /// What might still be replaceable.
    candidates: &'a HashMap<Name, Candidate>,
    /// What is not.
    escaped: HashSet<Name>,
    /// Whether a node the shared walker does not descend into was reached, in
    /// which case nothing survives — see the module doc.
    everything: bool,
    /// The name whose own initialiser is being scanned, if one is.
    ///
    /// # Why a read of it there is not the read this analysis models
    ///
    /// Because the binding does not exist yet. `let o = {a: 1, b: o.a}` is a
    /// `ReferenceError`: `o` is in its temporal dead zone until the whole
    /// declaration finishes, so `o.a` throws rather than answering `1`. The
    /// scan cannot see that from the shape of the node — an `o.a` inside the
    /// initialiser looks exactly like the `o.a` on the next line — so the
    /// position is carried instead.
    ///
    /// It costs a field rather than a traversal. The alternative was a second
    /// walk of the initialiser looking for the name, which is the shape this
    /// crate has removed six times.
    declaring: Option<Name>,
}

impl Escaping<'_> {
    /// Records that a name is reachable as a value, so nothing may replace it.
    fn kill(&mut self, name: Name) {
        self.escaped.insert(name);
    }

    /// The same, for whatever a pattern writes to.
    fn kill_pattern(&mut self, pattern: &Pattern) {
        let mut bound = Vec::new();
        pattern.bound_names(&mut bound);
        for name in bound {
            self.kill(name);
        }
    }

    /// Records an access this analysis models: `o.k`, read or written.
    fn note_use(&mut self, object: Name, property: Name, depth: u32, is_write: bool) {
        // In its own initialiser the name is dead, so this is not an access at
        // all — it is the throw the program is entitled to.
        if self.declaring == Some(object) {
            self.kill(object);
            return;
        }
        let Some(candidate) = self.candidates.get(&object) else {
            // Not a candidate, so `o.k` on it is an ordinary property access and
            // there is nothing to decide.
            return;
        };
        let known = candidate.keys.contains(&property);
        let declared_at = candidate.depth;
        // A key the literal did not write is not a key this analysis knows: it
        // would be read from the prototype chain, or answer `undefined`, and a
        // binding is neither.
        if !known || (is_write && depth > declared_at) {
            self.kill(object);
        }
    }
}

/// Kills every candidate a statement can reach.
fn scan_stmt(statement: &Stmt, depth: u32, state: &mut Escaping) {
    // `for (o of xs)` writes `o` once a pass, and the shared walker is silent
    // about a for-each target.
    if let StmtKind::ForEach {
        target: ForEachTarget::Assign(pattern),
        ..
    } = &statement.kind
    {
        state.kill_pattern(pattern);
    }

    let inner = depth + u32::from(is_barrier(statement));
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(nested) => scan_stmt(nested, inner, state),
        StmtChild::Expr(expr) => scan_expr(expr, inner, state, true),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                // The initialiser is scanned with the name being bound marked
                // dead, so a read of it in there refuses the replacement rather
                // than being taken for an ordinary access. Restored rather than
                // cleared: a declaration is not nested inside another one, but
                // saying so by construction costs nothing.
                let outer = match &binding.target {
                    Pattern::Name(name) => state.declaring.replace(*name),
                    _ => state.declaring.take(),
                };
                scan_expr(value, inner, state, true);
                state.declaring = outer;
            }
        }
        StmtChild::Catch(catch) => {
            for nested in &catch.body {
                scan_stmt(nested, inner, state);
            }
        }
        // What nested code can see is `capture::captured`'s answer, applied once
        // in [`analyse`]. Descending here would be a second, worse computation
        // of it — and the crude one is already the safe one.
        StmtChild::Function(_) | StmtChild::Class(_) => {}
    });
}

/// Kills every candidate an expression can reach.
///
/// `allow_member` says whether a `o.k` found *here* is a plain read. It is the
/// one thing the shared walker cannot tell a caller — a callee and an argument
/// are both `Child::Expr` — and it is false by default, so a position this file
/// has not reasoned about makes the read look like any other mention and kills
/// the candidate.
fn scan_expr(expr: &Expr, depth: u32, state: &mut Escaping, allow_member: bool) {
    match &expr.kind {
        // ALLOWED: a read of a key the literal wrote.
        ExprKind::Member {
            object,
            property,
            optional: false,
        } if allow_member => {
            if let ExprKind::Ident(name) = &object.kind {
                state.note_use(*name, *property, depth, false);
                return;
            }
        }

        // ALLOWED: a write of one. `o.k &&= v` is not here on purpose — the
        // operator performs no write when the left side decides, which the
        // emitter refuses on a property rather than getting subtly wrong, and
        // replacing the object would quietly make it compile.
        ExprKind::Assign {
            target: AssignTarget::Place(place),
            value,
            op,
        } if !matches!(op, AssignOp::Logical(_)) => {
            if let ExprKind::Member {
                object,
                property,
                optional: false,
            } = &place.kind
                && let ExprKind::Ident(name) = &object.kind
            {
                state.note_use(*name, *property, depth, true);
                scan_expr(value, depth, state, true);
                return;
            }
        }

        // The shared walker does not descend into these four. Its comment says
        // the emitter refuses them, and that is stale for at least the template
        // — `emit/template.rs` emits one. Rather than write the traversal it is
        // missing, nothing survives a body containing one: a node whose children
        // this analysis cannot see must not leave a candidate alive.
        ExprKind::Template { .. }
        | ExprKind::TaggedTemplate { .. }
        | ExprKind::SuperMember { .. }
        | ExprKind::SuperCall { .. } => {
            state.everything = true;
            return;
        }

        _ => {}
    }

    // Nodes with a mixed shape, where one child is not a value position. Each is
    // handled here rather than through the flag below, because the flag is one
    // bit for a whole node and these need two.
    match &expr.kind {
        // `o.m()` does not read `o` and discard it — it hands it over as the
        // receiver. So the callee is scanned with reads disallowed, which makes
        // the `o` under it an ordinary mention and kills it. The arguments are
        // values, and `f(o.a)` is fine.
        //
        // Scanned here rather than through the walker for a second reason: the
        // walker drops a spread argument, so `f(...o)` would never have reached
        // the identifier at all.
        ExprKind::Call {
            callee, arguments, ..
        }
        | ExprKind::New {
            callee, arguments, ..
        } => {
            scan_expr(callee, depth, state, false);
            for argument in arguments {
                match argument {
                    Spreadable::Single(value) | Spreadable::Spread(value) => {
                        scan_expr(value, depth, state, true)
                    }
                }
            }
            return;
        }

        // The walker drops a spread element too, so `[...o]` would leave `o`
        // untouched.
        ExprKind::Array { elements } => {
            for element in elements.iter().flatten() {
                match element {
                    Spreadable::Single(value) | Spreadable::Spread(value) => {
                        scan_expr(value, depth, state, true)
                    }
                }
            }
            return;
        }

        // `o.k++` reads and writes through a place. It could be modelled, and is
        // not: `Update` yields the old value for one spelling and the new one
        // for the other, and every case this file adds is another one to defend.
        ExprKind::Update { target, .. } => {
            scan_expr(target, depth, state, false);
            return;
        }

        // `delete o.k` removes a property, which a binding cannot be. `typeof`
        // is NOT here: it reads a value, and `typeof o.k` on a replaced property
        // is `typeof` of a local.
        ExprKind::Unary {
            op: UnaryOp::Delete,
            operand,
        } => {
            scan_expr(operand, depth, state, false);
            return;
        }

        // The place is where the write lands and the value is a value. The write
        // this analysis models returned above, so anything still here is a place
        // it does not model — `o = 2`, or `o.k &&= v` — and the place being
        // scanned with reads disallowed is what kills it.
        ExprKind::Assign { target, value, .. } => {
            match target {
                AssignTarget::Place(place) => scan_expr(place, depth, state, false),
                // Destructuring writes several names at once.
                AssignTarget::Pattern(pattern) => state.kill_pattern(pattern),
            }
            scan_expr(value, depth, state, true);
            return;
        }

        _ => {}
    }

    // Anything reached as a bare name is reachable as a whole object, whatever
    // the surrounding node turns out to be. This is the line that makes the
    // default "escapes": the two allowed shapes returned above, so an identifier
    // arriving here is a mention this analysis does not model.
    if let ExprKind::Ident(name) = &expr.kind {
        state.kill(*name);
    }

    // Which nodes hold values in every child. A node NOT named here has its
    // children scanned as if no read were allowed, which is the conservative
    // answer: the worst it can do is refuse a replacement.
    let children_are_values = matches!(
        &expr.kind,
        ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Sequence { .. }
            | ExprKind::Conditional { .. }
            | ExprKind::Object { .. }
            // `o.a.b` and `o.a[i]` read `o.a` as a value and then work on it,
            // so the child of a member or an index is a value position — the
            // object being `o` ITSELF is what the line above kills.
            | ExprKind::Member { .. }
            | ExprKind::Index { .. }
    );
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => scan_expr(inner, depth, state, children_are_values),
        Child::Function(_) | Child::Class(_) => {}
    });
}

/// The binding one replaced property lives in.
///
/// Minted through `Names` like any other, so the rest of the emitter cannot tell
/// it from a name the program wrote — which is the point: it merges, shadows and
/// stores by the same rules.
fn field_name(ctx: &mut Ctx, object: Name, property: Name) -> Name {
    let text = format!(
        "{MARKER}{}.{}",
        ctx.names.text(object),
        ctx.names.text(property)
    );
    ctx.names.intern(&text)
}

/// The binding a property access reads, when the access is one that was
/// replaced.
///
/// `None` means the ordinary path: an object exists and the property is on it.
pub(super) fn field_of(
    ctx: &mut Ctx,
    object: &Expr,
    property: Name,
    optional: bool,
) -> Option<Name> {
    // `o?.k` is a different question — whether `o` is absent — and the analysis
    // refuses a candidate it is written on, so this cannot be reached. Checked
    // anyway, because the two facts live in different files.
    if optional {
        return None;
    }
    let ExprKind::Ident(name) = &object.kind else {
        return None;
    };
    if !ctx.flattened.has(*name, property) {
        return None;
    }
    Some(field_name(ctx, *name, property))
}

/// Emits a replaced declaration, or answers that this one is not replaced.
///
/// The property values are emitted **in source order**, which is the whole of
/// the ordering obligation: each is evaluated once, in the order written, at the
/// point the literal was written.
pub(super) fn declare_flattened(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    object: Name,
    value: Option<&Expr>,
) -> EmitResult<bool> {
    if ctx.flattened.properties(object).is_none() {
        return Ok(false);
    }
    let Some(Expr {
        kind: ExprKind::Object { properties },
        ..
    }) = value
    else {
        return Ok(false);
    };

    // Taken apart before anything is emitted. A refusal discovered halfway
    // through would already have emitted some of the values, and answering
    // "not replaced" then would emit them a second time.
    let mut fields = Vec::with_capacity(properties.len());
    for property in properties {
        let Property::Value {
            key: PropertyKey::Named(name),
            value,
            ..
        } = property
        else {
            return Ok(false);
        };
        fields.push((*name, value));
    }

    for (property, written) in fields {
        let produced = super::expr::emit_expr(builder, scope, ctx, written)?;
        let field = field_name(ctx, object, property);
        super::binding::declare(builder, scope, ctx, field, produced)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Names;
    use crate::parse::parse_script;
    use crate::syntax::{FunctionBody, ModuleItem};

    /// The locals a function body would have its objects replaced in.
    ///
    /// Written against source rather than a hand-built tree, for the reason
    /// `proven.rs`'s sibling helper gives: the claim is about what a *program*
    /// means, and a tree assembled here could be one the parser never produces.
    fn replaced(source: &str) -> Vec<String> {
        let mut names = Names::default();
        let program =
            parse_script(&format!("function t() {{ {source} }}"), &mut names).expect("parses");
        let [ModuleItem::Stmt(statement)] = program.body.as_slice() else {
            panic!("one statement");
        };
        let StmtKind::Function(function) = &statement.kind else {
            panic!("a function");
        };
        let FunctionBody::Block(body) = &function.body else {
            panic!("a block");
        };
        // The same answer the emitter uses, from the same place, rather than an
        // empty set that would make every capture test pass for no reason.
        let captured = super::super::capture::captured(body, &[]);
        let decided = analyse(body, &[], &captured);
        // Re-interning is how a test turns a name back into text: interning is
        // idempotent, so asking for a spelling hands back the name it had.
        let mut found: Vec<String> = ["o", "p", "q"]
            .into_iter()
            .filter(|text| {
                let name = names.intern(text);
                decided.properties(name).is_some()
            })
            .map(str::to_owned)
            .collect();
        found.sort();
        found
    }

    #[test]
    fn an_object_read_and_written_by_name_never_has_to_exist() {
        // The case the whole file is for: three runtime calls and a heap cell
        // become three locals and one addition.
        assert_eq!(
            replaced("let o = {a: 1, b: 2}; o.a = o.a + o.b; return o.a;"),
            ["o"]
        );
    }

    #[test]
    fn returning_the_object_hands_it_to_a_caller_that_can_still_read_it() {
        assert!(replaced("let o = {a: 1}; return o;").is_empty());
    }

    #[test]
    fn passing_the_object_to_a_call_lets_the_callee_keep_it() {
        assert!(replaced("let o = {a: 1}; f(o); return o.a;").is_empty());
    }

    #[test]
    fn calling_a_method_on_the_object_passes_it_as_the_receiver() {
        // The subtle one: `o.m()` contains a member access that looks exactly
        // like a read, and the object is handed over as `this`. Reading it as a
        // read is what a position-blind allow-list would do.
        assert!(replaced("let o = {a: 1}; o.a(); return o.a;").is_empty());
    }

    #[test]
    fn storing_the_object_in_another_object_keeps_it_reachable() {
        // And the other half of the same line: `p` is still replaceable, because
        // what its property holds is now an ordinary local.
        assert_eq!(
            replaced("let o = {a: 1}; let p = {x: o}; return p.x;"),
            ["p"]
        );
    }

    #[test]
    fn a_nested_function_that_can_see_the_name_keeps_the_object_alive() {
        // Two activations of `g` share `o`, so it lives in an environment
        // object. There is nothing to replace it with.
        assert!(replaced("let o = {a: 1}; function g() { return o.a; } return o.a;").is_empty());
    }

    #[test]
    fn a_nested_function_mentioning_something_else_blocks_nothing() {
        // The over-approximation in `capture.rs` is by NAME, not by "a closure
        // exists", and this is what says so — a body full of closures does not
        // cost every local in the function its replacement.
        assert_eq!(
            replaced("let o = {a: 1}; function g() { return z; } return o.a;"),
            ["o"]
        );
    }

    #[test]
    fn a_key_the_compiler_cannot_resolve_could_name_any_property() {
        assert!(replaced("let o = {a: 1}; return o[k];").is_empty());
    }

    #[test]
    fn a_key_the_literal_never_wrote_is_not_one_of_the_bindings() {
        // `o.b` is `undefined` or something on the prototype chain. A binding is
        // neither, so the object stays an object.
        assert!(replaced("let o = {a: 1}; return o.b;").is_empty());
    }

    #[test]
    fn reassigning_the_local_puts_something_that_is_not_the_literal_in_it() {
        assert!(replaced("let o = {a: 1}; o = 2; return o.a;").is_empty());
    }

    #[test]
    fn a_property_value_that_calls_something_still_runs_in_source_order() {
        // `{a: f(), b: g()}` runs `f` then `g`, once each, and `declare_flattened`
        // emits them in that order at that point. This was refused until the
        // ordering argument was written down; refusing it cost 2.8x on the most
        // ordinary literal there is.
        assert_eq!(
            replaced("let o = {a: f(), b: g()}; return o.a + o.b;").len(),
            1
        );
    }

    #[test]
    fn a_property_value_that_names_another_candidate_lets_that_one_escape() {
        // `q` appears bare, which kills `q`. `p` is replaceable *because* `q` is
        // not — the dependency that makes this pass need no fixpoint.
        assert_eq!(replaced("let q = {a: 1}; let p = {x: q}; return p.x;"), ["p"]);
    }

    #[test]
    fn reading_the_object_inside_its_own_initialiser_is_the_dead_zone() {
        // `let o = {a: 1, b: o.a}` throws a ReferenceError: `o` is not bound
        // until the declaration finishes. Replacing it would answer `1` instead,
        // and the read looks exactly like the legal one on the next line — so
        // the position is carried rather than inferred.
        assert!(replaced("let o = {a: 1, b: o.a}; return o.b;").is_empty());
    }

    #[test]
    fn naming_the_object_inside_its_own_initialiser_is_refused_too() {
        // The same dead zone reached as a bare name rather than through a key.
        assert!(replaced("let o = {a: o}; return o.a;").is_empty());
    }

    #[test]
    fn a_write_inside_a_loop_would_need_a_carried_name_the_program_never_spells() {
        // The region rule. `loops.rs` turns "the body writes `x`" into a block
        // parameter by reading names out of the tree, and `o.a = 1` puts no name
        // there — correctly, since today it writes a heap property.
        assert!(replaced("let o = {a: 1}; while (c) { o.a = 2; } return o.a;").is_empty());
    }

    #[test]
    fn a_read_inside_a_loop_is_fine_because_it_defines_nothing() {
        // The other side of the same rule, and the reason it is about writes
        // rather than about mentions: a value defined before a loop dominates
        // every block in it.
        assert_eq!(
            replaced("let o = {a: 1}; while (c) { d(o.a); } return o.a;"),
            ["o"]
        );
    }

    #[test]
    fn an_object_whose_writes_stay_inside_the_loop_that_declares_it_is_replaced() {
        // Declared and written at the same depth, so the bindings are body-local
        // — a fresh one every pass, which is what the declaration means anyway.
        assert_eq!(
            replaced("while (c) { let o = {a: 1}; o.a = 2; d(o.a); }"),
            ["o"]
        );
    }

    #[test]
    fn writing_in_one_arm_of_an_if_is_merged_by_the_mechanism_that_already_exists() {
        // `if` is deliberately not a barrier: it merges by comparing scope
        // snapshots, which names nothing and so sees a replaced property exactly
        // as it sees any other local.
        assert_eq!(
            replaced("let o = {a: 1}; if (c) { o.a = 2; } return o.a;"),
            ["o"]
        );
    }

    #[test]
    fn a_spread_of_the_object_reads_however_many_properties_it_has() {
        // Reached through the argument list, which the shared walker drops — so
        // this pins the traversal this file adds rather than only the rule.
        assert!(replaced("let o = {a: 1}; f(...o); return o.a;").is_empty());
    }

    #[test]
    fn a_computed_key_in_the_literal_is_not_a_key_the_compiler_knows() {
        assert!(replaced("let o = {[k]: 1}; return o.a;").is_empty());
    }

    #[test]
    fn a_duplicate_key_would_leave_the_first_value_readable() {
        assert!(replaced("let o = {a: 1, a: 2}; return o.a;").is_empty());
    }

    #[test]
    fn deleting_a_property_needs_an_object_to_remove_it_from() {
        assert!(replaced("let o = {a: 1}; delete o.a; return o.a;").is_empty());
    }

    #[test]
    fn a_second_declaration_of_the_spelling_means_the_name_is_two_things() {
        assert!(replaced("let o = {a: 1}; { let o = 2; } return o.a;").is_empty());
    }
}
