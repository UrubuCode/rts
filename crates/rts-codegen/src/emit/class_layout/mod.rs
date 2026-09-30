//! A class as a LAYOUT: `new C(…)` that never has to construct anything.
//!
//! # What this buys, measured before it was written
//!
//! `const o = new F1(i); s += o.a` costs 80 ns where the same thing written as
//! `const o = { a: i }` costs 3.9 (release, 2026-09-29): the literal never
//! becomes an object — `escape` turns it into one local per property — and the
//! instance is allocated, constructed through the runtime and collected. The
//! class says nothing the literal does not. Its fields, their order and what is
//! written to each are all in the declaration, which is what a language with
//! layouts reads a class AS.
//!
//! # What is done, and what deliberately is not
//!
//! A declaration `const o = new C(a, b)` is REWRITTEN, in the body about to be
//! emitted, to the object literal `C`'s constructor would have filled — and
//! only where `escape` then proves that literal never has to exist. So this
//! file decides two things and `escape` decides the third:
//!
//! - which classes have a layout ([`layouts`]);
//! - which sites may use it ([`Layout::literal`]);
//! - whether the object is ever seen, which is `escape::analyse`'s one
//!   statement of that rule, asked of the rewritten body.
//!
//! The literal has no prototype, so handing it to anything that can look — a
//! call, a store, a method, `instanceof` — would hand over a different object;
//! every such use is a use `escape` refuses, which is what makes that rewrite
//! invisible.
//!
//! # An instance that escapes
//!
//! It has to exist, and it does not have to be CONSTRUCTED: `new C(a)` is
//! rewritten to `{ __proto__: C.prototype, k: a }`, the same fields under the
//! prototype `new` would have read. What that removes is the construction —
//! `construct`'s crossing, the constructor called through a pointer, and for a
//! derived class one of each per level — and what it keeps is the object, born
//! under its prototype with the type `new` would have given it. Measured,
//! release, 2026-09-29: 75 ns for a base class and 157 for a derived one,
//! against 48 for a literal.
//!
//! A class is written `C.prototype` here although it is never read as a value
//! in the program: the proof was taken of the program as written, and this read
//! is the one `new` makes.
//!
//! # Which class has a layout
//!
//! Every clause is a way the constructor could do something a literal does not:
//!
//! - **declared once in the program and never assigned** — `inline`'s pair of
//!   whole-program facts, for the same reason: the site names the class by its
//!   spelling;
//! - **`extends` only of a class that has a layout itself**, named bare. Then
//!   the instance is the parent's fields followed by its own, which is what a
//!   language with layouts makes of inheritance: `super(a, b)` is the parent's
//!   writes, given `a` and `b`, and it has to be the constructor's first
//!   statement because nothing may be written before it;
//! - **no accessor, no private or computed member**: `this.k = v` reaches a
//!   setter where a literal defines a property;
//! - **nothing can have put a setter on `Object.prototype`** — see
//!   [`setters_reachable`];
//! - **a field initialiser names nothing**: it is evaluated per instance with
//!   `this` bound, and a literal has neither that scope nor that receiver;
//! - **the constructor is a list of `this.k = value`**, each value a parameter
//!   or something that names nothing. No `return`, no call, no read of `this`.
//!
//! - **never read as a value** (`receiver::handed_over`): `C.prototype` reached
//!   at all is how a setter or another method gets installed from outside, and
//!   a class handed to a call can have anything done to it.
//!
//! # A method called on the instance
//!
//! `o.norm()` hands `o` to `norm`, which is a use `escape` refuses — so one
//! method call made the instance exist (92 ns against 4.5). Where the method is
//! writes to its own fields and then one expression over them, the call is
//! rewritten to exactly that — `(o.n = o.n + 1, o.n)` for `bump() { this.n =
//! this.n + 1; return this.n; }` — with `this.k` spelled `o.k`, and what is
//! left is field reads and field writes. A SEQUENCE and not statements spliced
//! in beside the call: a call is an expression wherever it is written, and the
//! language already has the expression that runs several things in order.
//!
//! `Math.sqrt(…)` may be part of it where the program leaves `Math` as the
//! language defines it, which is `Ctx::math_primordial`'s fact and the one
//! `emit/math` folds on.
//! `receiver.rs` substitutes the same call for a receiver that is an OBJECT;
//! this is the form for one that is not going to be.
//!
//! The arguments of such a call must be names or literals: the body evaluates
//! its fields and operators in its own order, and an argument that runs code
//! would run it at a different point.
//!
//! # Which site may use it
//!
//! The constructor's parameters are evaluated BEFORE its body; a literal
//! evaluates each value where it is written. So an argument is substituted only
//! when moving it cannot be seen: it is a name or a literal, or every parameter
//! is written to a field bare, once, in parameter order.
//!
//! # The read of the class that stays
//!
//! `new C()` before `C` holds its class raises, and a function declared above
//! the class and called before it is how a program gets there. The rewrite
//! keeps that: the statement is preceded by `C.prototype;`, which raises on
//! exactly the values `new` raises on here — a binding not yet written reads
//! `undefined` in this engine, and a member of `undefined` is a `TypeError` as
//! constructing it is. A bare read of `C` was written first and is not enough:
//! it answers `undefined` and carries on, so a construction that used to raise
//! built an object instead.
//!
//! Refusing every class not declared above all the code that runs was the
//! alternative, and it refuses most programs: a constant declared above a class
//! is code that runs.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use super::inline::Declarations;
use super::primordial::Disturbed;
use crate::names::{Name, Names};
use crate::syntax::{
    AssignOp, AssignTarget, Class, ClassElement, ClassKey, Expr, ExprKind, FunctionBody, Literal,
    MethodKind, Pattern, Property, PropertyKey, Spreadable, Stmt, StmtKind,
};
use crate::values::Singleton;

/// What one field is given.
#[derive(Clone)]
enum Given {
    /// The parameter at this position, bare.
    Parameter(usize),
    /// An expression that names nothing.
    Closed(Expr),
}

/// A method that is writes to its own fields, then one expression over them.
struct Method {
    parameters: Vec<Name>,
    /// Each an assignment to `this.k` or an update of it, in order.
    writes: Vec<Expr>,
    /// What it answers, or `None` where it answers `undefined`.
    answer: Option<Expr>,
}

/// A class whose instances are a list of fields and nothing else.
pub(super) struct Layout {
    parameters: usize,
    fields: Vec<(Name, Given)>,
    methods: BTreeMap<Name, Rc<Method>>,
    /// Whether every parameter is written to a field once, in order.
    in_order: bool,
}

mod proof;
mod rewrite;
mod setters;

use proof::{classes_in, inert, layout_of, spelled, undefined};
use rewrite::{Born, Instances, born_in_list, unseen};
pub(super) use setters::setters_reachable;

/// Every class in `body` that has a layout, by the name a site constructs it
/// under.
///
/// # A class declared inside a function
///
/// Is a different class on every call, and has the same layout on every one:
/// the fields and what is written to each are in the declaration, which does
/// not change. What differs is the prototype, and the rewrite reads that where
/// `new` would have — `C.prototype`, of whichever `C` is bound at the site.
///
/// Only the top of the program was read at first, and `bench/analytic.ts`
/// declares every class inside the function that uses it, so five commits of
/// layout moved none of its rows. Collecting from any depth is legal for the
/// reason `inline::candidates` gives for functions: the name is declared once
/// in the WHOLE program, so a site that spells it names this class or nothing.
pub(super) fn layouts(
    body: &[Stmt],
    declared: &Declarations,
    writes: &Disturbed,
    handed_over: &BTreeSet<Name>,
    names: &Names,
    math: Option<Name>,
) -> BTreeMap<Name, Rc<Layout>> {
    let mut declared_classes = Vec::new();
    for statement in body {
        classes_in(statement, &mut declared_classes);
    }
    let mut found = BTreeMap::new();
    for class in declared_classes {
        let Some(name) = class.name else {
            continue;
        };
        if declared.count(name) != 1 || !writes.untouched(name) || handed_over.contains(&name) {
            continue;
        }
        if let Some(layout) = layout_of(class, names, &found, math) {
            found.insert(name, Rc::new(layout));
        }
    }
    found
}

impl Layout {
    /// The literal `new C(arguments)` fills, or `None` where moving an argument
    /// could be seen.
    fn literal(&self, arguments: &[Spreadable], at: rts_cranelift::fault::Position) -> Option<Expr> {
        if self.fields.is_empty() {
            return None;
        }
        self.filled(arguments, at, None)
    }

    /// The instance itself: the same fields, born under `class.prototype`.
    fn born(
        &self,
        class: &Expr,
        prototype: Name,
        arguments: &[Spreadable],
        at: rts_cranelift::fault::Position,
    ) -> Option<Expr> {
        if self.fields.len() > INLINE_FIELDS {
            return None;
        }
        let under = Expr {
            kind: ExprKind::Member {
                object: Box::new(class.clone()),
                property: prototype,
                optional: false,
            },
            at,
        };
        self.filled(arguments, at, Some(under))
    }

    fn filled(
        &self,
        arguments: &[Spreadable],
        at: rts_cranelift::fault::Position,
        under: Option<Expr>,
    ) -> Option<Expr> {
        let mut written = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let Spreadable::Single(value) = argument else {
                return None;
            };
            written.push(value);
        }
        let all_inert = written.iter().all(|value| inert(value));
        if !all_inert && !(self.in_order && written.len() == self.parameters) {
            return None;
        }
        let properties = under
            .into_iter()
            .map(Property::Prototype)
            .chain(self.fields.iter().map(|(key, given)| Property::Value {
                key: PropertyKey::Named(*key),
                value: match given {
                    Given::Closed(value) => value.clone(),
                    Given::Parameter(position) => match written.get(*position) {
                        Some(value) => (*value).clone(),
                        None => undefined(at),
                    },
                },
                shorthand: false,
            }))
            .collect();
        Some(Expr {
            kind: ExprKind::Object { properties },
            at,
        })
    }
}

/// How many fields an object born under its prototype holds in its own cell —
/// `emit/object.rs`'s figure, for the literal this writes.
const INLINE_FIELDS: usize = 15;

impl Layout {
    /// What `object.name(arguments)` answers, as an expression over the fields.
    fn call(&self, object: Name, name: Name, arguments: &[Spreadable]) -> Option<Expr> {
        let method = self.methods.get(&name)?;
        let mut written = Vec::with_capacity(arguments.len());
        for argument in arguments {
            match argument {
                Spreadable::Single(value) if inert(value) => written.push(value),
                _ => return None,
            }
        }
        let answer = match &method.answer {
            Some(answer) => spelled(answer, method, &written, object),
            None => undefined(rts_cranelift::fault::Position::default()),
        };
        if method.writes.is_empty() {
            return Some(answer);
        }
        let at = answer.at;
        let operands = method
            .writes
            .iter()
            .map(|write| spelled(write, method, &written, object))
            .chain([answer])
            .collect();
        Some(Expr {
            kind: ExprKind::Sequence { operands },
            at,
        })
    }
}

/// `body` with every `const o = new C(…)` it may rewrite rewritten, or `None`
/// where there is nothing to rewrite.
///
/// Two passes, because the rewrite is only legal where the object is never
/// seen and only the rewritten body can be asked: every site is rewritten,
/// `escape` is asked, and the sites it refused are put back as written.
pub(super) fn rewritten(
    layouts: &BTreeMap<Name, Rc<Layout>>,
    body: &[Stmt],
    parameters: &[Name],
    prototype: Name,
    here: &Here,
) -> Option<Vec<Stmt>> {
    let none = BTreeMap::new();
    let layouts = match here.allowed {
        true => layouts,
        false => &none,
    };
    let with_local;
    let layouts = match local(layouts, body, here) {
        Some(found) => {
            with_local = found;
            &with_local
        }
        None => layouts,
    };
    if layouts.is_empty() && here.direct.is_none() {
        return None;
    }
    let settled = unseen(layouts, body, parameters, prototype);
    // Then every construction that is left, which is every instance that is
    // seen: born under its prototype rather than constructed — and every
    // `f.call(t, …)` and `f.apply(t, […])` on a function the program proves,
    // which is the call it spells.
    let born = Born {
        layouts,
        prototype,
        direct: here.direct.as_ref(),
    };
    // Asked BEFORE the body is taken out of it. It was asked after, which
    // answered "nothing was rewritten" for every body whose instances were all
    // unseen — and handed back the body as written. Every test passed; the
    // clock read 80 ns where the step before read 4.
    let mut changed = settled.is_some();
    let mut all = settled.unwrap_or_else(|| body.to_vec());
    let nothing = Instances::new();
    born_in_list(&mut all, &nothing, &born, &mut changed);
    changed.then_some(all)
}

/// What a body is asked about its own classes with.
pub(super) struct Here<'a> {
    /// Whether any class of the program may have a layout — `Ctx`'s fact.
    pub allowed: bool,
    pub eval: Name,
    pub global_this: Name,
    pub names: &'a Names,
    /// `Math`, where the program leaves it as the language defines it.
    pub math: Option<Name>,
    /// The functions `f.call(t, …)` and `f.apply(t, […])` may be spelled as a
    /// call of, or `None` where `Function.prototype` is not the language's.
    pub direct: Option<Direct<'a>>,
}

/// What lets `f.call(t, a)` be written `f(a)`: `f` is a function the whole
/// program proves and `inline` could substitute — declared once, never
/// assigned, and closed over its parameters, so it reads no `this` and the
/// receiver decides nothing. Both stages then see the call they already know
/// how to substitute; the running emitter's own door for these was measured
/// at 79 ns for `call` and 206 for `apply` against 1.5 for the call itself.
///
/// The receiver is evaluated by `call` and not by `f(a)`, so it is dropped only
/// where dropping it cannot be seen: a name, `this` or a literal.
pub(super) struct Direct<'a> {
    pub functions: &'a BTreeMap<Name, Rc<crate::emit::inline::Inlinable>>,
    /// The names the program reads as values. `inline`'s proof is about what
    /// `f` IS, and that is enough for `f(a)`; `f.call` also asks what `f`
    /// HAS, and a function handed to `Object.setPrototypeOf` may have any
    /// `call` at all. The corpus had that program: `call_apply_direct.test.ts`
    /// answered `2` where `"proto"` was expected on the build without this.
    pub handed_over: &'a BTreeSet<Name>,
    pub call: Name,
    pub apply: Name,
}

/// The program's layouts and those of the classes `body` declares for itself,
/// or `None` where it declares none with one.
///
/// # Why a second proof, and why it is the same proof
///
/// The program-wide one asks that a name be declared ONCE in the program, and
/// two functions that each declare a `class P` fail it — every row of
/// `bench/analytic.ts` that declares a class spells it `P` or `A`. But a class
/// declared at the top of a function is bound in that function and nowhere
/// else, so everything that can name it is inside this body: the three facts
/// are asked of the body, and they are the three the program is asked.
///
/// Only a class at the TOP of the body. One declared in an inner block is
/// bound in that block, and a site outside it spelling the same name means
/// something else.
fn local(
    layouts: &BTreeMap<Name, Rc<Layout>>,
    body: &[Stmt],
    here: &Here,
) -> Option<BTreeMap<Name, Rc<Layout>>> {
    let declared_here: Vec<&Class> = body
        .iter()
        .filter_map(|statement| match &statement.kind {
            StmtKind::Class(class) => Some(&**class),
            _ => None,
        })
        .filter(|class| class.name.is_some_and(|name| !layouts.contains_key(&name)))
        .collect();
    if declared_here.is_empty() {
        return None;
    }
    let declared = Declarations::of(body);
    let writes = super::primordial::disturbed(body, here.eval, here.global_this);
    let handed_over = super::receiver::handed_over(body);
    let mut found = layouts.clone();
    let before = found.len();
    for class in declared_here {
        let name = class.name.expect("filtered above");
        if declared.count(name) != 1 || !writes.untouched(name) || handed_over.contains(&name) {
            continue;
        }
        if let Some(layout) = layout_of(class, here.names, &found, here.math) {
            found.insert(name, Rc::new(layout));
        }
    }
    (found.len() > before).then_some(found)
}
