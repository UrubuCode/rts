//! The arithmetic and relational operators.
//!
//! # Why these are entry points at all
//!
//! `a - b` looks like arithmetic and is not, quite. It is
//! `ToNumber(ToPrimitive(a)) - ToNumber(ToPrimitive(b))`, and `ToNumber` of a
//! string reads that string's text out of the heap. So the membership rule
//! applies for the ordinary reason — *touches the heap* — and it is the
//! conversion rather than the subtraction that makes it true.
//!
//! Which is also why the subtraction itself is not interesting here. Once both
//! operands are numbers, every one of these is one machine instruction, and a
//! type pass that proves both sides are numbers should emit that instruction
//! instead of this call. These exist for the case where nothing was proved.
//!
//! # `+` is not here, and that is not an oversight
//!
//! It lives beside the others in [`super`] because it is the one operator of the
//! group that is not arithmetic: it decides between adding and concatenating
//! from what its operands turn out to be, and concatenating allocates. Every
//! operator in this file converts to a number and has no second answer.

use super::bigint_class::{Op, Relation};
use super::{Context, with_current};
use crate::coerce::{Relational, relational};
use crate::value::{Value, to_number};

/// `ToNumber`, including the part that needs the heap.
///
/// [`to_number`] answers for everything a machine word can settle and returns
/// `None` for a reference, because a reference might be a string — which
/// converts by reading its text — or an object, which converts by running a
/// `valueOf` and is therefore a call rather than a conversion.
///
/// This resolves the first case and leaves the second absent, which keeps the
/// distinction the caller needs: a string is finished here, an object is not.
pub(super) fn as_number(context: &Context, value: Value) -> Option<f64> {
    if let Some(number) = to_number(value, context.singletons) {
        return Some(number);
    }
    let text = context.text_at(value.as_slot()?)?;
    Some(crate::coerce::string_to_number(text))
}

/// Both operands as numbers, or `NaN` if either still needed `ToPrimitive`.
///
/// # Why `NaN` rather than a panic
///
/// The compiler is supposed to have resolved `ToPrimitive` before calling here,
/// because running a `valueOf` is calling user code and an entry point cannot do
/// that. Reaching this with an object is a contract violation.
///
/// `NaN` is what the operation would have produced for an object with no useful
/// `valueOf` anyway, so it is the answer least likely to be mistaken for a
/// correct one — and unlike a panic, it does not turn a compiler defect into a
/// dead process while the compiler is being written.
pub(super) fn operands(context: &Context, left: Value, right: Value) -> (f64, f64) {
    (
        as_number(context, left).unwrap_or(f64::NAN),
        as_number(context, right).unwrap_or(f64::NAN),
    )
}

/// The order every operator in this file converts its operands in.
///
/// Left then right, which is what `coerce::add_operand_order` states for `+`.
/// The same order, taken from the same place rather than written a second time:
/// the two have no reason to differ and every reason to be got wrong separately.
pub(super) fn operand_order() -> [crate::coerce::Side; 2] {
    crate::coerce::add_operand_order()
}

/// `a - b`.
#[rtse::entry]
pub fn subtract(left: u64, right: u64) -> u64 {
    if let Some(answer) = overload::binary(Overload::Sub, left, right) {
        return answer;
    }
    let (left, right) = super::primitive::operands(
        left,
        right,
        operand_order(),
        crate::coerce::Hint::Number,
    );
    // Asked in a borrow of ITS OWN, and `settled` after it ends. Mixing a bigint
    // with anything else is a `TypeError`, so this arm refuses now, and building
    // the error object borrows the context again — raising while `binary`'s
    // borrow is live aborts the process rather than failing, because the panic
    // cannot unwind through an entry point. Same shape as `bitwise.rs`.
    if let Some(outcome) =
        with_current(|context| super::bigint_class::binary(context, Op::Sub, left, right))
    {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(a - b).bits()
    })
}

/// `a * b`.
#[rtse::entry]
pub fn multiply(left: u64, right: u64) -> u64 {
    if let Some(answer) = overload::binary(Overload::Mul, left, right) {
        return answer;
    }
    ordinary_multiply(left, right)
}

/// `a * b` as the specification states it, with no `rts` operator consulted —
/// what [`multiply`] falls to and what [`unary_plus`] is.
fn ordinary_multiply(left: u64, right: u64) -> u64 {
    let (left, right) = super::primitive::operands(
        left,
        right,
        operand_order(),
        crate::coerce::Hint::Number,
    );
    if let Some(outcome) =
        with_current(|context| super::bigint_class::binary(context, Op::Mul, left, right))
    {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(a * b).bits()
    })
}

/// `a / b`.
///
/// No check for a zero divisor, and none is missing: JavaScript divides in
/// IEEE-754, where `1 / 0` is `Infinity` and `0 / 0` is `NaN`. A guard here
/// would be replacing the language's answer with a different one.
#[rtse::entry]
pub fn divide(left: u64, right: u64) -> u64 {
    if let Some(answer) = overload::binary(Overload::Div, left, right) {
        return answer;
    }
    let (left, right) = super::primitive::operands(
        left,
        right,
        operand_order(),
        crate::coerce::Hint::Number,
    );
    if let Some(outcome) =
        with_current(|context| super::bigint_class::binary(context, Op::Div, left, right))
    {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(a / b).bits()
    })
}

/// `a % b`.
///
/// Remainder, not modulo. The result takes the sign of the **dividend**, so
/// `-5 % 3` is `-2` and not `1`. Rust's `%` on `f64` is the same operation, so
/// this is a delegation rather than an implementation — worth saying, because
/// the two differ in most languages and agreeing here is luck that should be
/// recorded rather than relied on silently.
#[rtse::entry]
pub fn remainder(left: u64, right: u64) -> u64 {
    if let Some(answer) = overload::binary(Overload::Mod, left, right) {
        return answer;
    }
    let (left, right) = super::primitive::operands(
        left,
        right,
        operand_order(),
        crate::coerce::Hint::Number,
    );
    if let Some(outcome) =
        with_current(|context| super::bigint_class::binary(context, Op::Rem, left, right))
    {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(a % b).bits()
    })
}

/// `a % b` where the compiler already proved both operands are doubles.
///
/// # Why this exists beside [`remainder`], which computes the same thing
///
/// Not for the arithmetic — the last line of each is the same `%` on two
/// `f64`, and that is deliberate so the two cannot drift about sign or about
/// `NaN`. It exists for the SHAPE. [`remainder`] takes two tagged values,
/// coerces them (which can read the heap and run a `valueOf`), consults the
/// bigint path, and answers a tagged value; every one of those steps is
/// unreachable once the operands are known to be doubles, and the site still
/// pays two widenings, a narrowing and the thrown-value check that follows any
/// entry point able to run user code.
///
/// This one takes and answers unboxed doubles, touches no context, and cannot
/// throw — so no caller needs to ask whether it did.
///
/// # Why it is a call at all
///
/// Because the machine cannot do it. `crates/rts-cranelift`'s `NumOp` records
/// that a double remainder is `fmod` on every target here, and that the
/// identity which would avoid the call stops being exact past 2^53. A call is
/// what the machine floor is for this operation — the same call a native
/// program pays.
///
/// # No bigint path, and that is not an omission
///
/// A bigint is a reference, never a proven double, so no site that reaches
/// here can be holding one. [`remainder`] keeps that branch because its
/// operands are tagged and might be.
#[rtse::entry]
pub fn number_remainder(left: f64, right: f64) -> f64 {
    left % right
}

/// The shared body of the four relational operators.
///
/// # Why they are four entry points and not one with an operand
///
/// An operand would be a fifth argument at every call site, carrying a constant
/// the compiler already knows. Four symbols cost four rows in a table that is
/// read in one screen, and each call site passes only what varies.
fn compare(op: Relational, left: u64, right: u64) -> bool {
    // Each operator its own symbol, and no derivation: `>` is never asked as
    // the negation of an `le` the object happened to declare.
    let declared = match op {
        Relational::Less => Overload::Lt,
        Relational::LessEqual => Overload::Le,
        Relational::Greater => Overload::Gt,
        Relational::GreaterEqual => Overload::Ge,
    };
    if let Some(answer) = overload::binary(declared, left, right) {
        return overload::truth(answer);
    }
    // Outside the borrow, and in the operators own order: `a <= b` is specified
    // as `!(b < a)`, so it converts the RIGHT operand first. That is invisible
    // until an operand has a side-effecting `valueOf`, which is why the order
    // comes from `coerce` rather than being assumed to be left-to-right here.
    let (left, right) = super::primitive::operands(
        left,
        right,
        crate::coerce::relational_operand_order(op),
        crate::coerce::Hint::Number,
    );
    // Asked before the closures below, and before conversion: a bigint and a
    // number DO compare in the language — only *arithmetic* between them is
    // refused — but comparing them as doubles loses every value past 2^53, which
    // is the range a bigint exists for.
    //
    // In a borrow of its own, like the five arithmetic operators above: a
    // comparison cannot refuse today, and `settled` is shared with five that
    // can, so the borrow discipline is the same one everywhere rather than a
    // property of which operator is calling.
    let want = match op {
        Relational::Less => Relation::Less,
        Relational::LessEqual => Relation::LessEqual,
        Relational::Greater => Relation::Greater,
        Relational::GreaterEqual => Relation::GreaterEqual,
    };
    if let Some(outcome) =
        with_current(|context| super::bigint_class::binary(context, Op::Compare(want), left, right))
    {
        return Value(super::bigint_class::settled(outcome))
            .as_bool()
            .unwrap_or(false);
    }
    with_current(|context| {
        let text_of = |value: Value| {
            value
                .as_slot()
                .and_then(|slot| context.text_at(slot))
                .cloned()
        };
        // `None` is *unordered*, which is what `NaN` produces on either side.
        // All four operators read it as false — which is why `NaN <= NaN` is
        // false rather than the negation of `NaN > NaN` being true.
        // Both closures come from the same context: one reads a string's text,
        // the other converts one to a number. A relational operator needs both
        // because it decides between them from what the operands are.
        let number_of = |value: Value| as_number(context, value);
        relational(op, Value(left), Value(right), text_of, number_of).unwrap_or(false)
    })
}

/// `a < b`.
#[rtse::entry]
pub fn less(left: u64, right: u64) -> bool {
    compare(Relational::Less, left, right)
}

/// `a <= b`.
#[rtse::entry]
pub fn less_equal(left: u64, right: u64) -> bool {
    compare(Relational::LessEqual, left, right)
}

/// `a > b`.
#[rtse::entry]
pub fn greater(left: u64, right: u64) -> bool {
    compare(Relational::Greater, left, right)
}

/// `a >= b`.
#[rtse::entry]
pub fn greater_equal(left: u64, right: u64) -> bool {
    compare(Relational::GreaterEqual, left, right)
}

/// `-x`.
///
/// # Why unary minus is its own entry point
///
/// It was emitted as `x * -1`, which is exactly right for a double and exactly
/// wrong for a bigint: `-1` is a **number**, so a bigint operand made the
/// multiply a mixed operation — which the language refuses. So `-1n` was `NaN`,
/// and with it every negative literal, `BigInt.asIntN` reading back, and
/// `-1n & 3n`. It lived in `bigint_class.rs` for that reason, and moved here
/// when it became the one unary operator `rts`'s `operators.neg` can answer.
#[rtse::entry]
pub fn negate(value: u64) -> u64 {
    if let Some(answer) = overload::unary(Overload::Neg, value) {
        return answer;
    }
    // Um bigint e um numero ja pronto respondem dentro de UM emprestimo, que e
    // o caminho de toda a aritmetica compilada.
    enum Held {
        Big(crate::bigint::BigInt),
        Number(f64),
        /// Um objeto: `ToPrimitive` chama codigo do utilizador, e isso nao pode
        /// acontecer com o contexto emprestado.
        Ask,
    }
    let held = with_current(|context| {
        if let Some(held) = super::bigints::digits_of(context, value).map(crate::bigint::BigInt::neg) {
            return Held::Big(held);
        }
        match as_number(context, Value(value)) {
            Some(number) => Held::Number(number),
            None => Held::Ask,
        }
    });
    let number = match held {
        Held::Big(held) => return with_current(|context| context.bigint_value(held)),
        Held::Number(number) => number,
        // `-{valueOf(){return 5}}` e `-[]` passam por `ToPrimitive` fora do
        // emprestimo, como o `+x` unario ja fazia.
        Held::Ask => super::class_support::to_number(value),
    };
    Value::from_f64(-number).bits()
}

/// `+x`, and the `ToNumeric` read half of `x++` and `x--`.
///
/// # Why it exists beside [`multiply`], which answered both
///
/// The emitter spelled `+x` as `x * 1` and `x++` as `(x * 1) - -1`, which is
/// the same observable sequence for every value the language has — and stopped
/// being so the day `rts`'s `operators.mul` could answer a `*`. `+v` on an
/// object declaring `mul` would have called it with `1`. So this is the multiply
/// by one with the overload question left out and nothing else changed: the
/// same conversion, the same bigint refusal, the same bits.
#[rtse::entry]
pub fn unary_plus(value: u64) -> u64 {
    ordinary_multiply(value, Value::from_f64(1.0).bits())
}

#[path = "overload.rs"]
pub(super) mod overload;
use overload::Overload;

#[cfg(test)]
#[path = "operators_tests.rs"]
mod tests;

/// `a == b`.
///
/// # Why this waited for a client
///
/// Loose equality is the operation whose specification most often reaches
/// `ToPrimitive`, and `ToPrimitive` on an object runs a `valueOf` — user code,
/// which an entry point cannot call. So it was left out until something needed
/// it, rather than being written with the interesting half missing and looking
/// finished.
///
/// # What it does, and the one case it declines
///
/// Everything that does not need a call: two values of the same kind are
/// compared strictly, `null` and `undefined` are equal to each other and to
/// nothing else, and every remaining pair is compared **as numbers** — which is
/// what the specification's table reduces to once objects are set aside, since
/// a boolean converts to a number and a string compared against a number
/// converts too.
///
/// An object against a primitive is converted first, by
/// [`super::primitive::to_primitive`] — which is what "waited for a client"
/// above was waiting for. It used to answer `false`, so `[] == 0` and `[] == ""`
/// were both wrong.
///
/// **Only when exactly one side is an object.** Two objects compare by identity
/// and must never be converted: `{} == {}` is false, and a pair converted first
/// would compare `"[object Object]"` against itself and answer true.
#[rtse::entry]
pub fn loose_equals(left: u64, right: u64) -> bool {
    // Asked of two OBJECTS only, which `binary` enforces for `Eq`: so the
    // `null`/`undefined` arms below, and the emitter's settled `x == null`, are
    // untouched by it.
    if let Some(answer) = overload::binary(Overload::Eq, left, right) {
        return overload::truth(answer);
    }
    // Outside the borrow, and before anything else reads the operands: a
    // conversion runs user code. Guarded so the identity rule above survives.
    let (left_object, right_object, left_absent, right_absent) = with_current(|context| {
        let absent = |value: u64| {
            matches!(Value(value).kind(), crate::value::Kind::Singleton(number)
                if number == context.singletons.undefined
                    || number == context.singletons.null)
        };
        (
            super::primitive::is_object_in(context, left),
            super::primitive::is_object_in(context, right),
            absent(left),
            absent(right),
        )
    });
    // The specification's steps 2 to 4, and they come BEFORE step 10's
    // `ToPrimitive` rather than after it. Asked here rather than in the borrow
    // below, which is where it used to be asked and where it was too late:
    // `({ valueOf() {} }) == null` ran the `valueOf` and then discovered that
    // the other side was `null` and that no conversion had been needed.
    //
    // That was not merely wasted work — it is OBSERVABLE. Measured 2026-08-29
    // against node: `valueOf` was called twice per comparison where the
    // specification calls it zero times, so a program whose conversion counts,
    // logs or fetches behaved differently here. It also cost 1 456 ns against
    // the 8 the emitter now pays where it can settle the comparison itself.
    //
    // `null` and `undefined` are equal to each other and to NOTHING else, which
    // is why one absent side answers the whole question.
    if left_absent || right_absent {
        return left_absent && right_absent;
    }
    let hint = crate::coerce::Hint::Default;
    let (left, right) = match (left_object, right_object) {
        (true, false) => (super::primitive::to_primitive(left, hint), right),
        (false, true) => (left, super::primitive::to_primitive(right, hint)),
        _ => (left, right),
    };

    with_current(|context| {
        let (left, right) = (Value(left), Value(right));

        // The same rule again, and it is NOT dead code: the test above ran on
        // the operands as written, and `ToPrimitive` between here and there can
        // produce `undefined` — a `valueOf` that returns nothing. The
        // specification says so explicitly, by re-entering `IsLooselyEqual`
        // with the converted value rather than continuing down the table.
        let absent = |value: Value| {
            matches!(value.kind(), crate::value::Kind::Singleton(number)
                if number == context.singletons.undefined
                    || number == context.singletons.null)
        };
        if absent(left) || absent(right) {
            return absent(left) && absent(right);
        }

        // Same kind is strict equality, which also covers two strings (equal
        // when their text is) and two objects (equal when they are the same
        // one).
        if std::mem::discriminant(&left.kind()) == std::mem::discriminant(&right.kind()) {
            return crate::value::strict_equals(left, right, |a, b| context.same_text(a, b));
        }

        // Um bigint contra um numero, um booleano ou texto compara o valor
        // MATEMATICO dos dois lados. Nao estava aqui, entao caia no
        // `as_number`, que responde `None` para um bigint — e `1n == 1` era
        // falso. Converter os dois para `f64` teria sido mais curto e errado
        // no sitio que importa: `(2n ** 60n + 1n) == 2 ** 60` tem de ser falso,
        // e em double os dois lados sao o mesmo numero.
        let held = match (
            super::bigints::digits_of(context, left.bits()).cloned(),
            super::bigints::digits_of(context, right.bits()).cloned(),
        ) {
            (Some(held), None) => Some((held, right)),
            (None, Some(held)) => Some((held, left)),
            // Dois bigints ja foram respondidos pela igualdade estrita acima, e
            // nenhum dos dois nao e trabalho deste ramo.
            _ => None,
        };
        if let Some((held, other)) = held {
            return same_as_bigint(context, &held, other);
        }

        match (as_number(context, left), as_number(context, right)) {
            (Some(a), Some(b)) => a == b,
            // One of them is an object, so the answer needed `ToPrimitive`.
            _ => false,
        }
    })
}

/// Whether a bigint and a primitive of another kind name the same integer.
///
/// Text is parsed as a bigint rather than converted to a double, which is the
/// specification's `StringToBigInt` and the only reading that keeps
/// `9007199254740993n == "9007199254740993"` true — a double cannot hold that
/// number. A number that is not an integer, or is not finite, is equal to no
/// bigint at all, which is what `from_f64` answering `None` says.
fn same_as_bigint(context: &Context, held: &crate::bigint::BigInt, other: Value) -> bool {
    if let Some(cell) = other.as_slot()
        && let Some(text) = context.text_at(cell)
    {
        return text
            .to_rust()
            .and_then(|text| crate::bigint::BigInt::parse(text.trim()))
            .is_some_and(|parsed| held.cmp(&parsed) == std::cmp::Ordering::Equal);
    }
    as_number(context, other)
        .and_then(crate::bigint::BigInt::from_f64)
        .is_some_and(|converted| held.cmp(&converted) == std::cmp::Ordering::Equal)
}
