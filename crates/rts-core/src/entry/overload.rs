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
//! An overloaded `v + w` is ONE borrow and then the call: the objectness test,
//! the Get and the callable test share it. Measured before that (2026-09-18,
//! release, 2 000 000 iterations each allocating its result): `v.add(w)`
//! 146 ns, `v + w` 205 ns, and the gap was the crossings rather than the walk —
//! key conversion, the proxy question, the walk and the callable test each took
//! a `with_current` of their own.
//!
//! # Reuse-check
//!
//! The protocol is [`super::super::primitive::to_primitive`]'s
//! `Symbol.toPrimitive` step with a different key: an ordinary Get, a callable
//! test, a call with no borrow held, and rule 8 of this crate's README after
//! each of the two user-visible steps. The symbols come from
//! [`super::super::symbol`]'s shared table rather than a table of their own,
//! because a symbol's number is that module's.
//!
//! The Get is [`resolve`] — the walk `get_indexed` and `get_property` both end
//! in, and the one `instanceof` already probes the same way in `functions.rs`.
//! What `get_indexed` does BEFORE that walk cannot fire here: the key is a
//! symbol, so it is no array index, typed-array index or string element and not
//! `length`; and the receiver is an object, so the primitive fallback does not
//! apply. The two shapes that can run user code — a proxy's trap and an
//! accessor's getter — still take [`super::super::get_indexed`] with no borrow
//! held, so they keep the door they had.

use super::super::accessor::{Found, resolve};
use super::super::objects::undefined_of;
use super::super::primitive::is_object_in;
use super::super::{Context, with_current};
use crate::object::Key;
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

/// How many there are — the width of [`Operators`].
pub(in crate::entry) const COUNT: usize = 12;

/// The names the namespace publishes, indexed by `Overload as usize`.
///
/// Pinned against the enum by [`tests::every_operator_has_its_own_name`], since
/// the two are one list written in two places.
const NAMES: [&str; COUNT] = [
    "add", "sub", "mul", "div", "mod", "pow", "lt", "le", "gt", "ge", "eq", "neg",
];

/// The twelve symbols, and the property key each one resolves to.
///
/// The key is the number [`super::super::symbol::key_of`] memoises on the
/// symbol — the same one `get_indexed` reaches through `property_key` — so the
/// probe below and the generic Get name the same property by construction.
/// Kept here because reaching it through the symbol is a borrow and a table
/// read per operation that can never answer anything new: a symbol's key text
/// is fixed when it is minted.
pub(in crate::entry) struct Operators {
    symbols: [u64; COUNT],
    keys: [Key; COUNT],
}

impl Operators {
    fn at(&self, op: Overload) -> (u64, Key) {
        (self.symbols[op as usize], self.keys[op as usize])
    }
}

/// `rts`'s `operators` namespace, and the moment overloading becomes possible.
///
/// The table is kept BOXED in `Context::operators`, so the field is one word.
/// Inline, the twelve symbols alone added 104 bytes to `Context` and moved the
/// fields every call reads: `Math.min` measured +26 % for it (2026-09-18,
/// release, isolated loop), the same cause as the tail-call record
/// `tail_call.rs` moved out — and the keys beside them would only add to it.
/// They stay a FIELD rather than a thread-local because they live as long as
/// the context, and a context can be installed more than once.
///
/// Called by whatever builds the `rts` module. Filling
/// [`Context::operators`] is what arms every check in this file, so a host
/// that never calls this has a runtime in which no operator consults anything.
pub fn operators_namespace(context: &mut Context) -> u64 {
    let mut symbols = [0u64; COUNT];
    for (at, name) in NAMES.iter().enumerate() {
        symbols[at] = super::super::symbol::shared(
            context,
            format!("{}rts.operators.{name}", super::super::symbol::PREFIX),
            Some(format!("rts.operators.{name}")),
        );
    }
    let keys = symbols.map(|symbol| {
        super::super::symbol::key_of(context, symbol).expect("a symbol just minted has a key")
    });
    context.operators = Some(Box::new(Operators { symbols, keys }));
    let namespace = super::super::make_namespace(context, &[]);
    for (name, symbol) in NAMES.iter().zip(symbols) {
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
/// The left side's lookup shares the borrow that classifies the operands. The
/// right side's is NOT taken in it too, although that would save a borrow on
/// `2 * v`: the left's Get may run a getter, which is user code and may change
/// what the right would find, so the right is looked up only after the left
/// has answered nothing — the order the generic path always had.
///
/// A throw from the getter or the method answers `Some`, so the caller returns
/// at once and the compiled site above re-raises (rule 8).
pub(in crate::entry) fn binary(op: Overload, left: u64, right: u64) -> Option<u64> {
    let reference = |value: u64| matches!(Value(value).kind(), Kind::Reference(_));
    if !reference(left) && !reference(right) {
        return None;
    }
    let (symbol, key, found, right_object, absent) = with_current(|context| {
        let (symbol, key) = context.operators.as_deref()?.at(op);
        let left_object = is_object_in(context, left);
        let right_object = is_object_in(context, right);
        if op == Overload::Eq && !(left_object && right_object) {
            return None;
        }
        let found = if left_object {
            probe(context, key, left)
        } else {
            Probe::Missing
        };
        Some((symbol, key, found, right_object, undefined_of(context)))
    })?;
    let unreflected = Value::from_bool(false).bits();
    if let Some(answer) = answer(found, symbol, left, [right, unreflected], absent) {
        return Some(answer);
    }
    if !right_object {
        return None;
    }
    let found = with_current(|context| probe(context, key, right));
    answer(
        found,
        symbol,
        right,
        [left, Value::from_bool(true).bits()],
        absent,
    )
}

/// Unary `-` when the operand declared it: `obj[neg]()`.
pub(in crate::entry) fn unary(op: Overload, value: u64) -> Option<u64> {
    if !matches!(Value(value).kind(), Kind::Reference(_)) {
        return None;
    }
    let (symbol, found, absent) = with_current(|context| {
        let (symbol, key) = context.operators.as_deref()?.at(op);
        if !is_object_in(context, value) {
            return None;
        }
        Some((symbol, probe(context, key, value), undefined_of(context)))
    })?;
    answer(found, symbol, value, [absent, absent], absent)
}

/// A relational or equality answer, through `ToBoolean` — rule 3 of the
/// document: the method may answer anything, the operator answers a boolean.
pub(in crate::entry) fn truth(answer: u64) -> bool {
    !super::super::throw::in_flight()
        && with_current(|context| super::super::to_boolean_in(context, answer))
}

/// What one borrow could settle about `receiver[symbol]`.
enum Probe {
    /// A callable data property, read without running anything.
    Method(u64),
    /// Absent along the whole chain, or present and not callable: either way
    /// the operand declared nothing, and the answer is the specification's.
    Missing,
    /// Only user code can answer — a getter, or a proxy somewhere — so the
    /// generic Get has to, with no borrow held.
    Slowly,
}

/// `receiver[key]` inside the caller's borrow, for a receiver already known
/// to be an object.
///
/// `any_proxy` rather than asking whether THIS receiver is one: a proxy in the
/// receiver's prototype chain answers through its trap as well, and `resolve`
/// would walk straight past it. While no proxy exists anywhere neither can be
/// the case; once one does, every operand takes the generic path, which is
/// correct and merely as slow as it was before.
fn probe(context: &mut Context, key: Key, receiver: u64) -> Probe {
    if context.any_proxy() {
        return Probe::Slowly;
    }
    let Some(slot) = Value(receiver).as_slot() else {
        return Probe::Slowly;
    };
    match resolve(context, slot, key) {
        Found::Absent => Probe::Missing,
        Found::Value(held) if super::super::is_callable_in(context, held) => Probe::Method(held),
        Found::Value(_) => Probe::Missing,
        // A getter is user code and cannot run inside this borrow.
        Found::Getter(_) => Probe::Slowly,
    }
}

/// `receiver[symbol](first, second)` once a [`Probe`] is in hand, with no
/// borrow held: the method is user code whose first act may be to call the
/// runtime.
///
/// For [`Probe::Slowly`] the Get and the callable test are the generic ones,
/// and each is followed by what rule 8 asks: a Get that threw answers `Some`
/// of whatever it returned, so the caller hands the throw back instead of
/// calling.
fn answer(
    found: Probe,
    symbol: u64,
    receiver: u64,
    [first, second]: [u64; 2],
    absent: u64,
) -> Option<u64> {
    let method = match found {
        Probe::Method(method) => method,
        Probe::Missing => return None,
        Probe::Slowly => {
            let method = super::super::get_indexed(receiver, symbol);
            if super::super::throw::in_flight() {
                return Some(method);
            }
            if !with_current(|context| super::super::is_callable_in(context, method)) {
                return None;
            }
            method
        }
    };
    Some(super::super::call(
        method, receiver, first, second, absent, absent,
    ))
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
