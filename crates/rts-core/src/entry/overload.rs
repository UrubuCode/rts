//! Operator overloading, by symbols only `rts` provides.
//!
//! `import { operators } from "rts"` hands a program twelve symbols, and an
//! object that carries a callable under one of them answers the operator it
//! names: `class Vec2 { [operators.add](other, reversed) { … } }` makes
//! `a + b` call it. `docs/engine/operator-overloading.md` is the whole rule.
//!
//! # Why a symbol and not the method name
//!
//! The engine this one replaced turned `a + b` into `a.add(b)` whenever a
//! method called `add` existed. A `Set` has one, so `set + ""` stopped being
//! text — an ordinary program changed meaning because of a name it never chose
//! for this. A symbol nothing but `rts` can hand out is a declaration no
//! program makes by accident: **an object that did not ask is answered by the
//! specification, bit for bit.**
//!
//! Not `Symbol.for` either, for the same reason one level up: the global
//! registry is reachable by spelling a string, so a program could hold the key
//! without ever importing `rts`. These live in the shared table under a key
//! text `Symbol.for` cannot mint (`@@rts.operators.add`, where the registry
//! writes `@@for:…`), so they are made once per runtime and reached only
//! through the namespace.
//!
//! # Where the question is asked, and what it costs
//!
//! Only in the branch where an operand is an OBJECT, before `ToPrimitive` —
//! `docs/codegen/entry-tax.md` part five is why the order is the whole point.
//! Two primitives never reach a borrow here: the first test is on the tag.
//!
//! And [`Context::operators`] is `None` until the `rts` module is BUILT, which
//! `rts-std` does lazily, the first time a program imports it. So a program
//! that never imports `rts` pays one borrow and one `Option` test per object
//! operand and nothing else — which is also exactly correct rather than merely
//! cheap: with no symbol in existence, no object can carry one.
//!
//! # Reuse-check
//!
//! The protocol is [`super::super::primitive::to_primitive`]'s
//! `Symbol.toPrimitive` step with a different key: an ordinary Get
//! ([`super::super::get_indexed`]), a callable test, a call with no borrow
//! held, and rule 8 of this crate's README after each of the two user-visible
//! steps. The symbols come from [`super::super::symbol`]'s shared table rather
//! than a table of their own, because a symbol's number is that module's.

use super::super::objects::undefined_of;
use super::super::primitive::is_object_in;
use super::super::{Context, with_current};
use crate::value::{Kind, Value};

/// One operator a program may overload, in the order [`NAMES`] spells them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::entry) enum Overload {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Mod,
    /// `**`
    Pow,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`, and `!=` as its negation. Never `===`.
    Eq,
    /// Unary `-`.
    Neg,
}

/// How many there are — the width of [`Context::operators`].
pub(in crate::entry) const COUNT: usize = 12;

/// The names the namespace publishes, indexed by `Overload as usize`.
///
/// Pinned against the enum by [`tests::every_operator_has_its_own_name`], since
/// the two are one list written in two places.
const NAMES: [&str; COUNT] = [
    "add", "sub", "mul", "div", "mod", "pow", "lt", "le", "gt", "ge", "eq", "neg",
];

/// `rts`'s `operators` namespace, and the moment overloading becomes possible.
///
/// Called by whatever builds the `rts` module. Filling
/// [`Context::operators`] is what arms every check in this file, so a host
/// that never calls this has a runtime in which no operator consults anything.
pub fn operators_namespace(context: &mut Context) -> u64 {
    let mut made = [0u64; COUNT];
    for (at, name) in NAMES.iter().enumerate() {
        made[at] = super::super::symbol::shared(
            context,
            format!("{}rts.operators.{name}", super::super::symbol::PREFIX),
            Some(format!("rts.operators.{name}")),
        );
    }
    context.operators = Some(made);
    let namespace = super::super::make_namespace(context, &[]);
    for (name, symbol) in NAMES.iter().zip(made) {
        super::super::put_member(context, namespace, name, symbol);
    }
    namespace
}

/// The TypeScript for the namespace above, for `rts emit-types`.
///
/// Generated from [`NAMES`], the list the namespace is built from, so the
/// declaration cannot name an operator the runtime does not publish. Each is a
/// `unique symbol` because that is what lets `[operators.add](…) {}` type-check
/// as a method name — the form `lib.es2015` gives `Symbol.iterator`.
pub fn operators_declaration() -> String {
    let mut out = String::from(
        "// `import { operators } from \"rts\"` — operator overloading, opt-in.\n\
         // `tsc` still reports `a + b` between two classes as an error: TypeScript\n\
         // has no syntax for declaring an overload, and RTS runs the program\n\
         // because it does not type-check it. See docs/engine/operator-overloading.md.\n\
         declare module \"rts\" {\n  export interface RtsOperators {\n",
    );
    for name in NAMES {
        out.push_str(&format!("    readonly {name}: unique symbol;\n"));
    }
    out.push_str("  }\n  export const operators: RtsOperators;\n}\n\n");
    out
}

/// `a OP b` when a side declared the operator, or `None` for the
/// specification's path.
///
/// The left operand is asked first and answers `a[sym](b, false)`; the right
/// answers `b[sym](a, true)` only when the left did not, which is what lets
/// `2 * v` reach `v`. `==` asks only when BOTH are objects, so `x == null` and
/// the emitter's settled arms for it never get here with anything to find.
///
/// A throw from the getter or the method answers `Some`, so the caller returns
/// at once and the compiled site above re-raises (rule 8).
pub(in crate::entry) fn binary(op: Overload, left: u64, right: u64) -> Option<u64> {
    let reference = |value: u64| matches!(Value(value).kind(), Kind::Reference(_));
    if !reference(left) && !reference(right) {
        return None;
    }
    let (symbol, left_object, right_object) = with_current(|context| {
        let symbol = context.operators?[op as usize];
        Some((symbol, is_object_in(context, left), is_object_in(context, right)))
    })?;
    if op == Overload::Eq && !(left_object && right_object) {
        return None;
    }
    if left_object && let Some(answer) = dispatch(symbol, left, right, false) {
        return Some(answer);
    }
    if right_object && let Some(answer) = dispatch(symbol, right, left, true) {
        return Some(answer);
    }
    None
}

/// Unary `-` when the operand declared it: `obj[neg]()`.
pub(in crate::entry) fn unary(op: Overload, value: u64) -> Option<u64> {
    if !matches!(Value(value).kind(), Kind::Reference(_)) {
        return None;
    }
    let symbol = with_current(|context| {
        let symbol = context.operators?[op as usize];
        is_object_in(context, value).then_some(symbol)
    })?;
    let method = super::super::get_indexed(value, symbol);
    if super::super::throw::in_flight() {
        return Some(method);
    }
    let (callable, absent) =
        with_current(|context| (super::super::is_callable_in(context, method), undefined_of(context)));
    callable.then(|| super::super::call(method, value, absent, absent, absent, absent))
}

/// A relational or equality answer, through `ToBoolean` — rule 3 of the
/// document: the method may answer anything, the operator answers a boolean.
pub(in crate::entry) fn truth(answer: u64) -> bool {
    !super::super::throw::in_flight()
        && with_current(|context| super::super::to_boolean_in(context, answer))
}

/// `receiver[symbol](other, reversed)`, if the property is callable.
///
/// Every step runs with no borrow held: the Get may reach a getter and the call
/// reaches the method, and both are user code whose first act may be to call
/// the runtime.
fn dispatch(symbol: u64, receiver: u64, other: u64, reversed: bool) -> Option<u64> {
    let method = super::super::get_indexed(receiver, symbol);
    if super::super::throw::in_flight() {
        return Some(method);
    }
    let (callable, absent) =
        with_current(|context| (super::super::is_callable_in(context, method), undefined_of(context)));
    if !callable {
        return None;
    }
    let flag = Value::from_bool(reversed).bits();
    Some(super::super::call(method, receiver, other, flag, absent, absent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operator_has_its_own_name() {
        // `Overload as usize` indexes `NAMES` and `Context::operators`; an enum
        // reordered without the list would hand `a - b` the `add` method.
        let all = [
            (Overload::Add, "add"),
            (Overload::Sub, "sub"),
            (Overload::Mul, "mul"),
            (Overload::Div, "div"),
            (Overload::Mod, "mod"),
            (Overload::Pow, "pow"),
            (Overload::Lt, "lt"),
            (Overload::Le, "le"),
            (Overload::Gt, "gt"),
            (Overload::Ge, "ge"),
            (Overload::Eq, "eq"),
            (Overload::Neg, "neg"),
        ];
        assert_eq!(all.len(), COUNT);
        for (op, name) in all {
            assert_eq!(NAMES[op as usize], name);
        }
    }

    #[test]
    fn the_declaration_names_every_operator_as_a_unique_symbol() {
        let text = operators_declaration();
        for name in NAMES {
            assert!(
                text.contains(&format!("readonly {name}: unique symbol;")),
                "`[operators.{name}]` would not type-check as a method name:\n{text}"
            );
        }
        assert!(text.contains("declare module \"rts\""));
    }
}
