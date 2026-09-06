//! The operators, and what each one does to what it is given.
//!
//! Kept apart from the tree because an operator is a decision and a node is a
//! shape. Almost every question a lowering asks — does this coerce, can this
//! throw, does this evaluate both sides, can the result still be a 32-bit
//! integer — is a question about the operator, and it should have one answer
//! rather than one per place that asks.
//!
//! Grouped by what the language does with them, not by how they are spelled.
//! `<` and `instanceof` are written the same way and are not the same kind of
//! thing: one orders two values, the other asks a question of an object and
//! throws when it is not given one.

/// An operator with two operands, both always evaluated.
///
/// The short-circuiting operators are [`LogicalOp`] and are deliberately not in
/// this list — "evaluates both operands" is the property this type is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum BinaryOp {
    /// `+` — addition or concatenation, and it does not decide until it runs.
    ///
    /// Both operands go through ToPrimitive first, and the branch is chosen by
    /// what *came back*: `[] + {}` concatenates, though neither operand is a
    /// string. Deciding from the operand types before coercion is a different
    /// algorithm that happens to agree on the easy cases.
    Add,
    /// `-`.
    Sub,
    /// `*`.
    Mul,
    /// `/`.
    Div,
    /// `%` — the sign follows the dividend, so it is not a modulo.
    Rem,
    /// `**` — right-associative, and its left operand cannot be an unparenthesised
    /// unary expression: `-2 ** 2` does not parse. The grammar refuses to guess
    /// which reading was meant, and so does this compiler.
    Exponent,

    /// `&` — both operands through ToInt32.
    BitAnd,
    /// `|` — both operands through ToInt32.
    BitOr,
    /// `^` — both operands through ToInt32.
    BitXor,
    /// `<<` — left operand ToInt32, shift count masked to 5 bits.
    Shl,
    /// `>>` — arithmetic, sign-propagating.
    Shr,
    /// `>>>` — logical. The left operand goes through **ToUint32**, so the result
    /// ranges to 2³²−1 and does *not* fit a signed 32-bit representation.
    /// The one bitwise operator whose result can be wider than its inputs.
    UShr,

    /// `===`.
    StrictEqual,
    /// `!==`.
    StrictNotEqual,
    /// `==` — coerces first, by rules with no shorter description than
    /// themselves.
    LooseEqual,
    /// `!=`.
    LooseNotEqual,

    /// `<`.
    Less,
    /// `<=` — defined as `!(b < a)`, which coerces the *right* operand first.
    LessEqual,
    /// `>` — defined as `b < a`, likewise reversed.
    Greater,
    /// `>=` — defined as `!(a < b)`.
    GreaterEqual,

    /// `in` — asks whether a property key is reachable on an object.
    /// TypeError if the right operand is not an object.
    In,
    /// The `for`-`in` guard, and **no program can write it**.
    ///
    /// Minted by `emit/foreach.rs` when it expands a `for`-`in` into a counted
    /// loop over a snapshot of the keys: the body must not visit a key it has
    /// itself deleted, so each pass asks whether the key is still reachable.
    ///
    /// It was [`BinaryOp::In`], and that is the one thing this variant exists to
    /// stop being. The operator refuses a receiver that is not an object; a
    /// `for`-`in` converts one with `ToObject` at the loop's head and enumerates
    /// it. The two agreed only for as long as `in` answered `false` instead of
    /// raising — the day it started raising, `for (const k in "ab")` ended the
    /// program.
    ///
    /// A variant here rather than `Object(src)` around the operand, which is the
    /// same question written in the language: the expansion would then depend on
    /// a global name the program is free to shadow, and every `__rts_` name in
    /// this file is minted precisely so that it cannot.
    ForInHas,
    /// `instanceof` — walks the prototype chain, or defers to
    /// `Symbol.hasInstance` when the right operand defines it.
    /// TypeError if the right operand is not callable.
    InstanceOf,
}

impl BinaryOp {
    /// Whether both operands being numbers makes this arithmetic.
    ///
    /// `+` is included: on two numbers it adds, and the only reason it is
    /// interesting is that it does something else otherwise.
    pub fn is_arithmetic(self) -> bool {
        matches!(
            self,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Rem
                | BinaryOp::Exponent
        )
    }

    /// Whether this reduces both operands to 32 bits before operating.
    ///
    /// The reason this is worth asking as one question: every one of these
    /// discards everything a double could hold beyond 32 bits, whatever the
    /// operands were. `2**32 | 0` is `0`.
    pub fn is_bitwise(self) -> bool {
        matches!(
            self,
            BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
                | BinaryOp::Shl
                | BinaryOp::Shr
                | BinaryOp::UShr
        )
    }

    /// Whether the result of this bitwise operator still fits a signed 32-bit
    /// value.
    ///
    /// True of every one except `>>>`, which produces an unsigned 32-bit result.
    /// Answering `true` here for `>>>` is how `(-1) >>> 0` becomes `-1` instead
    /// of `4294967295` — a wrong number rather than a crash, which is the kind
    /// this crate exists to not produce.
    pub fn bitwise_result_fits_i32(self) -> bool {
        debug_assert!(self.is_bitwise(), "only meaningful for a bitwise operator");
        !matches!(self, BinaryOp::UShr)
    }

    /// Whether this compares for equality.
    pub fn is_equality(self) -> bool {
        matches!(
            self,
            BinaryOp::StrictEqual
                | BinaryOp::StrictNotEqual
                | BinaryOp::LooseEqual
                | BinaryOp::LooseNotEqual
        )
    }

    /// Whether this orders two values.
    ///
    /// `in` and `instanceof` are *not* here. They share a precedence level with
    /// the ordering operators and nothing else: they take an object on the right
    /// and throw when they do not get one.
    pub fn is_ordering(self) -> bool {
        matches!(
            self,
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual
        )
    }

    /// Whether this interrogates an object and throws when the right operand is
    /// not one.
    pub fn is_object_test(self) -> bool {
        matches!(self, BinaryOp::In | BinaryOp::InstanceOf)
    }

    /// Whether this converts its operands before comparing them.
    ///
    /// The distinction `===` exists for. Note this is about *equality*: the
    /// ordering operators coerce too, through ToPrimitive, which is a different
    /// question with a different answer for the same operands.
    pub fn converts(self) -> bool {
        matches!(self, BinaryOp::LooseEqual | BinaryOp::LooseNotEqual)
    }

    /// Whether this operator coerces its **right** operand first.
    ///
    /// `a <= b` is specified as `!(b < a)` and `a > b` as `b < a`, so the
    /// ToPrimitive calls happen in the reverse of the written order. Invisible
    /// until both operands have a side-effecting `valueOf`, at which point it is
    /// the whole answer.
    pub fn coerces_right_first(self) -> bool {
        matches!(self, BinaryOp::LessEqual | BinaryOp::Greater)
    }

    /// Whether this operator can be spelled as a compound assignment.
    ///
    /// The arithmetic and bitwise ones can; comparing and then assigning the
    /// comparison is not a thing the language offers, so `a ===b` has no
    /// compound form and `a <= b` has none either.
    pub fn has_compound_form(self) -> bool {
        self.is_arithmetic() || self.is_bitwise()
    }
}

/// An operator with one operand.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum UnaryOp {
    /// `-`.
    Negate,
    /// `+` — **not** an identity. It is ToNumber spelled short, so `+"3"` is `3`
    /// and `+1n` throws: unary plus is the one arithmetic operator BigInt
    /// refuses.
    Plus,
    /// `!` — asks whether the operand is falsy and answers the opposite.
    Not,
    /// `~` — ToInt32, then complement.
    BitNot,
    /// `typeof` — the only read in the language that cannot fail. On a name
    /// that does not exist it answers `"undefined"` where every other read
    /// throws.
    TypeOf,
    /// `void` — evaluates and discards.
    Void,
    /// `delete` — removes a property and answers whether the object now lacks
    /// it. Answers `true` for things that were never properties. Two early
    /// errors: an unqualified identifier in strict code, and a private field.
    Delete,
}

impl UnaryOp {
    /// Whether the operand is read as a value at all.
    ///
    /// `delete` and `typeof` take a *reference*, not a value — which is exactly
    /// why `typeof missing` does not throw and `delete obj.x` can address a
    /// property rather than what it holds.
    pub fn reads_a_value(self) -> bool {
        !matches!(self, UnaryOp::Delete | UnaryOp::TypeOf)
    }

    /// Whether this coerces its operand to a number.
    pub fn coerces_to_number(self) -> bool {
        matches!(self, UnaryOp::Negate | UnaryOp::Plus | UnaryOp::BitNot)
    }
}

/// An operator that may not evaluate its right operand.
///
/// Separate from [`BinaryOp`] because they do not evaluate both sides.
/// Modelling them as ordinary binary operators would put a special case in
/// every lowering that walks one, to undo a shape that was wrong to begin with.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum LogicalOp {
    /// `&&` — the left, if it is falsy; otherwise the right.
    And,
    /// `||` — the left, if it is truthy; otherwise the right.
    Or,
    /// `??` — the left, unless it is null or undefined.
    ///
    /// Not `||` with a different threshold: it distinguishes absent from falsy,
    /// which is the entire reason it was added. And it cannot be written
    /// unparenthesised beside `&&` or `||` — `a ?? b || c` does not parse,
    /// because the grammar declined to invent a precedence for it.
    Coalesce,
}

impl LogicalOp {
    /// Whether this operator may be written next to `op` without parentheses.
    ///
    /// Only `??` refuses, and it refuses in both directions.
    pub fn may_sit_beside(self, other: LogicalOp) -> bool {
        !matches!(
            (self, other),
            (LogicalOp::Coalesce, LogicalOp::And)
                | (LogicalOp::Coalesce, LogicalOp::Or)
                | (LogicalOp::And, LogicalOp::Coalesce)
                | (LogicalOp::Or, LogicalOp::Coalesce)
        )
    }
}

/// `++` or `--`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum UpdateOp {
    /// `++`.
    Increment,
    /// `--`.
    Decrement,
}

/// Whether an update is written before or after its operand.
///
/// Not cosmetic: it decides what the expression evaluates to. And in both cases
/// the target is read, coerced through ToNumeric, and written back — so
/// `let x = "5"; x++` leaves `x` as the number `6` and yields the number `5`,
/// never the string it started as.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum UpdatePosition {
    /// `++x` — yields the new value.
    Prefix,
    /// `x++` — yields the old value, already coerced.
    ///
    /// A restricted production: a line break between the operand and the
    /// operator ends the statement instead, so `a\n++b` is two statements.
    Postfix,
}

/// What an assignment does before it stores.
///
/// Three cases rather than an optional operator, because the third behaves
/// unlike the other two: a logical assignment may not evaluate its right side
/// **and may not assign at all**, where a compound assignment always does both.
/// An `Option<BinaryOp>` cannot hold that difference, and every reader would
/// have had to know it anyway.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum AssignOp {
    /// `=` — the only form whose target may be a destructuring pattern.
    Plain,
    /// `+=`, `**=`, `>>>=`, … — reads the target once, applies, stores.
    ///
    /// Once. `a[i()] += 1` calls `i` a single time, which is why this is carried
    /// as an operator rather than rewritten to `a[i()] = a[i()] + 1`.
    Compound(BinaryOp),
    /// `&&=`, `||=`, `??=` — reads the target, and stops if the target already
    /// decided. No read of the right side, and no write.
    ///
    /// Observable: `obj.x ||= f()` does not call `f` and performs no `[[Set]]`
    /// when `obj.x` is truthy — so a setter on `x` does not run.
    Logical(LogicalOp),
}

impl AssignOp {
    /// The compound form of `op`, if the language spells one.
    ///
    /// Returns `None` for the comparing operators, which have no compound form.
    /// Constructing the variant directly is possible in this crate and not
    /// outside it — the invariant is enforced here rather than described
    /// somewhere a caller has to find.
    pub fn compound(op: BinaryOp) -> Option<Self> {
        op.has_compound_form().then_some(AssignOp::Compound(op))
    }

    /// Whether the target may be a destructuring pattern.
    ///
    /// Only `=`. `[a, b] += c` and `[a] ||= b` are both syntax errors, for
    /// different reasons that arrive at the same rule.
    pub fn allows_pattern_target(self) -> bool {
        matches!(self, AssignOp::Plain)
    }

    /// Whether this always stores something.
    ///
    /// False only for the logical forms, which is the property they exist for.
    pub fn always_assigns(self) -> bool {
        !matches!(self, AssignOp::Logical(_))
    }

    /// Whether the right operand is always evaluated.
    pub fn always_evaluates_value(self) -> bool {
        self.always_assigns()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_shift_is_the_one_bitwise_result_that_does_not_fit_i32() {
        for op in [
            BinaryOp::BitAnd,
            BinaryOp::BitOr,
            BinaryOp::BitXor,
            BinaryOp::Shl,
            BinaryOp::Shr,
        ] {
            assert!(op.bitwise_result_fits_i32(), "{op:?}");
        }
        assert!(
            !BinaryOp::UShr.bitwise_result_fits_i32(),
            "(-1) >>> 0 is 4294967295, which is not a signed 32-bit value"
        );
    }

    #[test]
    fn in_and_instanceof_are_neither_ordering_nor_equality() {
        for op in [BinaryOp::In, BinaryOp::InstanceOf] {
            assert!(op.is_object_test());
            assert!(!op.is_ordering(), "they order nothing");
            assert!(!op.is_equality());
            assert!(!op.is_arithmetic());
        }
    }

    #[test]
    fn the_reversed_comparisons_coerce_their_right_operand_first() {
        assert!(BinaryOp::LessEqual.coerces_right_first());
        assert!(BinaryOp::Greater.coerces_right_first());
        assert!(!BinaryOp::Less.coerces_right_first());
        assert!(!BinaryOp::GreaterEqual.coerces_right_first());
    }

    #[test]
    fn comparing_operators_have_no_compound_form() {
        assert!(AssignOp::compound(BinaryOp::Add).is_some());
        assert!(AssignOp::compound(BinaryOp::UShr).is_some());
        assert!(AssignOp::compound(BinaryOp::Exponent).is_some());
        assert!(AssignOp::compound(BinaryOp::StrictEqual).is_none());
        assert!(AssignOp::compound(BinaryOp::Less).is_none());
        assert!(AssignOp::compound(BinaryOp::InstanceOf).is_none());
    }

    #[test]
    fn a_logical_assignment_may_do_nothing_at_all() {
        let logical = AssignOp::Logical(LogicalOp::Or);
        assert!(!logical.always_assigns());
        assert!(!logical.always_evaluates_value());
        assert!(!logical.allows_pattern_target());

        let compound = AssignOp::Compound(BinaryOp::Add);
        assert!(compound.always_assigns(), "a += b always stores");
        assert!(!compound.allows_pattern_target(), "[a, b] += c is an error");

        assert!(AssignOp::Plain.allows_pattern_target());
        assert!(AssignOp::Plain.always_assigns());
    }

    #[test]
    fn coalesce_cannot_be_written_beside_the_other_two() {
        assert!(!LogicalOp::Coalesce.may_sit_beside(LogicalOp::Or));
        assert!(!LogicalOp::Or.may_sit_beside(LogicalOp::Coalesce));
        assert!(LogicalOp::And.may_sit_beside(LogicalOp::Or));
        assert!(
            LogicalOp::Coalesce.may_sit_beside(LogicalOp::Coalesce),
            "a ?? b ?? c is fine — the ban is on mixing, not on repeating"
        );
    }

    #[test]
    fn typeof_and_delete_take_a_reference_rather_than_a_value() {
        assert!(!UnaryOp::TypeOf.reads_a_value());
        assert!(!UnaryOp::Delete.reads_a_value());
        assert!(UnaryOp::Not.reads_a_value());
        assert!(UnaryOp::Plus.coerces_to_number(), "unary + is ToNumber");
    }
}
