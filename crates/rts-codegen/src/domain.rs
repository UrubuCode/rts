//! This language's half of the shared IR: its types, its primitives, and what a
//! guard asserts.
//!
//! `rts-mir` owns the structure and never names a language; everything semantic
//! arrives through [`rts_mir::Domain`] and the two tables here. Its README rules 1
//! and 2 are the binding text, and `tests/toy_domain.rs` there is a second
//! implementation of this same trait — which is what keeps the parameterisation
//! from being decorative.
//!
//! # The three axes where this domain differs from that one
//!
//! Each was measured as *not neutral* in `docs/engine/a-second-language.md`, and
//! each is a place a shared lattice would have had to pick a winner:
//!
//! | | here | the toy domain |
//! |---|---|---|
//! | numbers | ONE type; [`Type::Int32`] refines [`Type::Double`] | two types, no supertype |
//! | falsy | seven values, so a number's truth is undecidable from its type | two values, so a number is always true |
//! | `join(int, float)` | `Double` | `Anything` |
//!
//! The first row is the one to read twice. This language has a single numeric
//! type and `Int32` is an *optimisation* of it, so joining the two narrows to the
//! wider of the two rather than giving up. In the other language they are
//! unrelated types and joining them gives up. Same trait, opposite answer, and
//! neither is `rts-mir`'s business.
//!
//! # Why the effect of an operation depends on its operands
//!
//! `a + b` on two proven `Int32`s reads nothing, allocates nothing and cannot
//! fail. The same `a + b` on two unknowns may call `valueOf` — user code, which
//! does everything a program can do — and may throw. So the effect is not a
//! property of the primitive; it is a property of the primitive *applied to these
//! types*, which is what [`Js::effect_of`] answers and what makes a guard worth
//! emitting at all: narrowing a type is how an operation becomes movable.
//!
//! `docs/codegen/entry-tax.md` part five is the class of defect this makes
//! visible — a conversion performed before the dispatch that would not have
//! needed it.

use rts_mir::cfg::{Const, EntryId, Prim};
use rts_mir::guard::Assertion;
use rts_mir::{Domain, Effect};

/// What this language knows about a value.
///
/// A flat set rather than a tree of refinements, because the only refinement
/// relation it has is `Int32` under `Double` and encoding one relation as a
/// hierarchy costs more than [`Js::join`] spelling it out.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Type {
    /// No path arrives.
    Nothing,
    /// `undefined`.
    Undefined,
    /// `null`.
    Null,
    /// A boolean, known where the program wrote one.
    Bool(Option<bool>),
    /// A number that fits in a signed 32-bit integer.
    ///
    /// Not a type of the language — the language has one numeric type — but a
    /// proof about a value of it, which is what lets an addition become a machine
    /// add instead of a runtime call.
    Int32,
    /// A number.
    Double,
    /// A string.
    Str,
    /// An object whose layout is known, by the machine's shape id.
    ///
    /// Carried as a plain number because a shape id is minted by
    /// `rts_cranelift::shape` and this type travels into `rts-mir`, which may not
    /// name a machine type.
    Shaped(u32),
    /// An object of unknown layout.
    Object,
    /// Something callable.
    Callable,
    /// Nothing is known.
    Anything,
}

/// A primitive this language declares.
///
/// The IR carries a [`Prim`] index; this is what the index means, here and
/// nowhere else. Adding one is adding a row to [`Js::PRIMS`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JsPrim {
    /// `a + b`, which adds numbers or concatenates strings, and may call
    /// `valueOf` to find out which.
    Add,
    /// `a - b`. Numeric whatever it is given, and it may coerce to find out.
    Subtract,
    /// `a * b`, on the same terms.
    Multiply,
    /// `a / b`, which answers a double even of two integers.
    Divide,
    /// `a % b`, whose answer keeps the sign of the left operand.
    Remainder,
    /// `a < b`, which also coerces and also answers a boolean.
    LessThan,
    /// `a === b`, which coerces nothing. The one comparison that cannot call
    /// user code, which is why it is a row of its own.
    StrictEquals,
    /// `typeof a`, which answers a string and reads nothing.
    TypeOf,
    /// `!a`, which reads this language's truth rule.
    Not,
    /// Reading a property whose position a shape decided.
    FieldRead,
    /// Writing one.
    FieldWrite,
    /// This language's truth rule, as an operation.
    ///
    /// A branch needs a machine boolean and a value of this language is not one,
    /// so the conversion is an operation rather than something the lowering
    /// performs on the way past. It calls nothing: `ToBoolean` inspects a value
    /// and never reaches `valueOf`, which is what separates it from every
    /// arithmetic row above.
    Truthy,
}

/// What a guard of this language asserts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JsAssertion {
    /// The value is a number that fits in an `i32`.
    IsInt32,
    /// The value is a number.
    IsDouble,
    /// The value is a string.
    IsStr,
    /// The value is an object of this shape.
    HasShape(u32),
}

/// This language's domain: the tables, and the trait over them.
///
/// Holds no state beyond the assertion table because the primitive table is
/// static — a primitive is a row of source code here, not something a program
/// registers. Shapes are not: `HasShape` carries an id the machine minted while
/// compiling this program, so those accumulate.
#[derive(Clone, Debug, Default)]
pub struct Js {
    assertions: Vec<JsAssertion>,
}

impl Js {
    /// Every primitive, in the order their [`Prim`] indices run.
    ///
    /// The index IS the position in this table, so a row may be appended and
    /// never inserted. `rts-mir` rule 4: the IR treats the number as opaque, so
    /// nothing outside this file may depend on which number a primitive has —
    /// including, deliberately, the next language's table.
    const PRIMS: &'static [JsPrim] = &[
        JsPrim::Add,
        JsPrim::Subtract,
        JsPrim::Multiply,
        JsPrim::Divide,
        JsPrim::Remainder,
        JsPrim::LessThan,
        JsPrim::StrictEquals,
        JsPrim::TypeOf,
        JsPrim::Not,
        JsPrim::FieldRead,
        JsPrim::FieldWrite,
        JsPrim::Truthy,
    ];

    /// An empty domain.
    pub fn new() -> Self {
        Self::default()
    }

    /// The index the IR carries for a primitive.
    pub fn prim(&self, which: JsPrim) -> Prim {
        let at = Self::PRIMS
            .iter()
            .position(|held| *held == which)
            .expect("every JsPrim is a row of PRIMS");
        Prim(at as u32)
    }

    /// What a [`Prim`] index means.
    pub fn meaning(&self, prim: Prim) -> Option<JsPrim> {
        Self::PRIMS.get(prim.0 as usize).copied()
    }

    /// Registers an assertion, answering the index a guard carries.
    ///
    /// Interned rather than appended blindly, so that two guards asserting the
    /// same thing carry the same index — which is what lets CSE see them as one.
    pub fn assertion(&mut self, what: JsAssertion) -> Assertion {
        match self.assertions.iter().position(|held| *held == what) {
            Some(at) => Assertion(at as u32),
            None => {
                self.assertions.push(what);
                Assertion(self.assertions.len() as u32 - 1)
            }
        }
    }

    /// What an [`Assertion`] index means.
    pub fn asserted(&self, assertion: Assertion) -> Option<JsAssertion> {
        self.assertions.get(assertion.0 as usize).copied()
    }

    /// What an operation does besides answer, given what its operands are.
    ///
    /// The header of this module says why this takes the operand types. The short
    /// of it: coercion is what calls user code, and a proof that no coercion is
    /// needed is what makes the operation movable.
    pub fn effect_of(&self, prim: Prim, args: &[Type]) -> Effect {
        let Some(which) = self.meaning(prim) else {
            // An index this table does not hold: assume the worst, which is the
            // only safe answer and is unreachable through `Self::prim`.
            return Effect::CALLS_USER.and(Effect::THROWS).and(Effect::WRITES);
        };
        match which {
            // Coercion is the whole question. `valueOf` and `toString` are user
            // code, so an operand that might need either makes this a call.
            JsPrim::Add
            | JsPrim::Subtract
            | JsPrim::Multiply
            | JsPrim::Divide
            | JsPrim::Remainder
            | JsPrim::LessThan => match args.iter().all(Self::needs_no_coercion) {
                true => match which {
                    // Concatenation allocates even when nothing coerces.
                    JsPrim::Add if args.iter().any(|held| *held == Type::Str) => {
                        Effect::ALLOCATES
                    }
                    _ => Effect::PURE,
                },
                false => Effect::CALLS_USER.and(Effect::THROWS),
            },
            // Neither reads the heap nor coerces.
            JsPrim::StrictEquals | JsPrim::TypeOf | JsPrim::Not | JsPrim::Truthy => Effect::PURE,
            // A shaped read is a load at a known offset; an unshaped one goes
            // through the runtime, which may run a getter.
            JsPrim::FieldRead => match args.first() {
                Some(Type::Shaped(_)) => Effect::READS,
                _ => Effect::READS.and(Effect::CALLS_USER).and(Effect::THROWS),
            },
            JsPrim::FieldWrite => match args.first() {
                Some(Type::Shaped(_)) => Effect::WRITES,
                _ => Effect::WRITES.and(Effect::CALLS_USER).and(Effect::THROWS),
            },
        }
    }

    /// Whether coercing a value of this type can run user code.
    ///
    /// # Why the answer is about OBJECTS and not about numbers
    ///
    /// The obvious version of this returned true only for numbers, and it was
    /// wrong in a direction that costs speed rather than correctness — which is
    /// the direction that never fails a test. `ToNumber` of a string parses it,
    /// `ToNumber` of `undefined` is `NaN`, and neither is user code: the
    /// specification's only step that calls anything a program wrote is
    /// `ToPrimitive` on an **object**, which reaches `valueOf` and `toString`.
    ///
    /// So a `"2" * "3"` coerces twice and calls nothing, and marking it a call
    /// would refuse every motion of it for ever. `docs/codegen/entry-tax.md` part
    /// five is the same observation from the other side: the specification puts
    /// cheap arms ahead of the conversion, and writing the conversion first is
    /// the natural way to get it wrong.
    fn needs_no_coercion(of: &Type) -> bool {
        matches!(
            of,
            Type::Int32
                | Type::Double
                | Type::Bool(_)
                | Type::Str
                | Type::Undefined
                | Type::Null
        )
    }

    /// Whether every value of this type is a number.
    fn is_numeric(of: &Type) -> bool {
        matches!(of, Type::Int32 | Type::Double)
    }
}

impl Domain for Js {
    type Type = Type;

    fn top(&self) -> Type {
        Type::Anything
    }

    fn bottom(&self) -> Type {
        Type::Nothing
    }

    fn join(&self, left: &Type, right: &Type) -> Type {
        match (left, right) {
            (Type::Nothing, other) | (other, Type::Nothing) => other.clone(),
            (one, two) if one == two => one.clone(),
            (Type::Bool(_), Type::Bool(_)) => Type::Bool(None),
            // THE AXIS. One numeric type, and `Int32` is a proof about a value of
            // it, so the join is the wider proof rather than a giving-up. The
            // toy domain's answer here is `Anything`, and both are right about
            // their own language.
            (one, two) if Self::is_numeric(one) && Self::is_numeric(two) => Type::Double,
            // Two shapes are two layouts with nothing in common but being
            // objects. A shape TREE relation would say more, and it is the
            // machine's rather than this domain's — asking it here would put a
            // machine query inside a type lattice.
            (Type::Shaped(_) | Type::Object, Type::Shaped(_) | Type::Object) => Type::Object,
            _ => Type::Anything,
        }
    }

    fn of_const(&self, value: &Const) -> Type {
        match value {
            Const::Int(held) => match i32::try_from(*held) {
                Ok(_) => Type::Int32,
                Err(_) => Type::Double,
            },
            Const::Float(_) => Type::Double,
            Const::Bool(truth) => Type::Bool(Some(*truth)),
            // The language's own constants: index zero is `undefined` and one is
            // `null`, which is `values::Singleton`'s numbering and the reason
            // that numbering is the language's to choose.
            Const::Declared(0) => Type::Undefined,
            Const::Declared(1) => Type::Null,
            Const::Declared(_) => Type::Anything,
        }
    }

    fn transfer(&self, prim: Prim, args: &[Type]) -> Type {
        let Some(which) = self.meaning(prim) else {
            return Type::Anything;
        };
        match which {
            // `+` is two operations wearing one spelling, and which one it is
            // depends on the operands. A string on either side concatenates.
            JsPrim::Add => match args {
                [one, two] if *one == Type::Str || *two == Type::Str => Type::Str,
                // TWO int32s DO NOT make an int32, and this is the subtlety worth
                // the comment: the sum may not fit, and this language has no
                // wrapping addition — `2147483647 + 1` is `2147483648`, a
                // double. Answering `Int32` here would be a wrong answer at the
                // one value that matters, so a range analysis is what recovers
                // it, not this table.
                [one, two] if Self::is_numeric(one) && Self::is_numeric(two) => Type::Double,
                _ => Type::Anything,
            },
            JsPrim::Subtract | JsPrim::Multiply | JsPrim::Remainder => match args {
                [one, two] if Self::is_numeric(one) && Self::is_numeric(two) => Type::Double,
                // Every arithmetic operator but `+` coerces to a number, so the
                // answer is a number whatever the operands were — including when
                // it is `NaN`, which is a number.
                _ => Type::Double,
            },
            // Division answers a double even of two integers, and `1/0` is
            // `Infinity` rather than a fault.
            JsPrim::Divide => Type::Double,
            JsPrim::LessThan | JsPrim::StrictEquals | JsPrim::Not => Type::Bool(None),
            // Folded where the type decides it, which is what `truth_of` is for:
            // an object is always true and `undefined` always false, so a branch
            // over either is a branch a later pass can remove. A number and a
            // string are never folded here, because zero, `NaN` and `""` are
            // values of them — the seven falsy cases, from the one place that
            // knows about them.
            JsPrim::Truthy => match args.first().and_then(|held| self.truth_of(held)) {
                Some(known) => Type::Bool(Some(known)),
                None => Type::Bool(None),
            },
            JsPrim::TypeOf => Type::Str,
            JsPrim::FieldRead => Type::Anything,
            // A write answers the value written, which is what makes `a = b = 1`
            // work.
            JsPrim::FieldWrite => args.get(1).cloned().unwrap_or(Type::Anything),
        }
    }

    fn of_entry(&self, _entry: EntryId) -> Type {
        // Per-entry answers are a table this does not have yet. `Anything` is the
        // sound half of not knowing, and the entry table is where the other half
        // will come from — `rts-host/src/entries.rs` already holds the shapes.
        Type::Anything
    }

    fn narrow(&self, assertion: Assertion, of: &Type) -> Type {
        match self.asserted(assertion) {
            Some(JsAssertion::IsInt32) => Type::Int32,
            Some(JsAssertion::IsDouble) => match of {
                // A guard cannot make a proof weaker: asserting "is a number" of
                // something already proved `Int32` leaves it `Int32`.
                Type::Int32 => Type::Int32,
                _ => Type::Double,
            },
            Some(JsAssertion::IsStr) => Type::Str,
            Some(JsAssertion::HasShape(shape)) => Type::Shaped(shape),
            None => of.clone(),
        }
    }

    fn truth_of(&self, of: &Type) -> Option<bool> {
        match of {
            Type::Undefined | Type::Null => Some(false),
            Type::Bool(known) => *known,
            // SEVEN FALSY VALUES is why these are undecidable from the type
            // alone: `0`, `-0`, `NaN` and `""` are values of `Int32`, `Double`
            // and `Str`. The toy domain answers `Some(true)` for all three,
            // because its language has two falsy values and a zero is true.
            Type::Int32 | Type::Double | Type::Str => None,
            // An object is truthy whatever it holds, and so is a function.
            Type::Shaped(_) | Type::Object | Type::Callable => Some(true),
            Type::Nothing | Type::Anything => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The axis. One numeric type here, two there.
    #[test]
    fn an_int32_joined_with_a_double_is_a_double() {
        let domain = Js::new();
        assert_eq!(domain.join(&Type::Int32, &Type::Double), Type::Double);
        assert_eq!(domain.join(&Type::Int32, &Type::Int32), Type::Int32);
        assert_eq!(domain.join(&Type::Int32, &Type::Str), Type::Anything);
    }

    #[test]
    fn bottom_is_the_identity_of_the_join() {
        let domain = Js::new();
        assert_eq!(domain.join(&Type::Nothing, &Type::Str), Type::Str);
        assert_eq!(domain.join(&Type::Str, &Type::Nothing), Type::Str);
        assert_eq!(domain.join(&Type::Nothing, &Type::Nothing), Type::Nothing);
    }

    /// The seven falsy values, as the one thing they imply about a type.
    #[test]
    fn a_number_and_a_string_have_no_decidable_truth_here() {
        let domain = Js::new();
        assert_eq!(domain.truth_of(&Type::Int32), None);
        assert_eq!(domain.truth_of(&Type::Double), None);
        assert_eq!(domain.truth_of(&Type::Str), None);
        assert_eq!(domain.truth_of(&Type::Undefined), Some(false));
        assert_eq!(domain.truth_of(&Type::Null), Some(false));
        assert_eq!(domain.truth_of(&Type::Object), Some(true));
        assert_eq!(domain.truth_of(&Type::Callable), Some(true));
        assert_eq!(domain.truth_of(&Type::Bool(Some(true))), Some(true));
        assert_eq!(domain.truth_of(&Type::Bool(None)), None);
    }

    /// The defect a table written the obvious way would ship: two int32s added
    /// are not an int32 at the one value that matters.
    #[test]
    fn two_int32s_added_are_a_double_because_the_sum_may_not_fit() {
        let domain = Js::new();
        let add = domain.prim(JsPrim::Add);
        assert_eq!(
            domain.transfer(add, &[Type::Int32, Type::Int32]),
            Type::Double
        );
        assert_eq!(domain.transfer(add, &[Type::Str, Type::Int32]), Type::Str);
        assert_eq!(domain.transfer(add, &[Type::Int32, Type::Str]), Type::Str);
    }

    #[test]
    fn division_answers_a_number_whatever_it_was_given() {
        let domain = Js::new();
        let divide = domain.prim(JsPrim::Divide);
        assert_eq!(
            domain.transfer(divide, &[Type::Int32, Type::Int32]),
            Type::Double
        );
        assert_eq!(
            domain.transfer(divide, &[Type::Anything, Type::Anything]),
            Type::Double
        );
    }

    /// What a guard buys, which is the only reason a specialised tier knows more.
    #[test]
    fn a_guard_narrows_and_never_widens() {
        let mut domain = Js::new();
        let is_int32 = domain.assertion(JsAssertion::IsInt32);
        let is_double = domain.assertion(JsAssertion::IsDouble);
        assert_eq!(domain.narrow(is_int32, &Type::Anything), Type::Int32);
        // Asserting the weaker fact about the stronger one keeps the stronger.
        assert_eq!(domain.narrow(is_double, &Type::Int32), Type::Int32);
        assert_eq!(domain.narrow(is_double, &Type::Anything), Type::Double);
    }

    /// Two guards asserting one thing must be one assertion, or CSE cannot see
    /// them as the same.
    #[test]
    fn an_assertion_is_interned() {
        let mut domain = Js::new();
        let first = domain.assertion(JsAssertion::HasShape(4));
        let again = domain.assertion(JsAssertion::HasShape(4));
        let other = domain.assertion(JsAssertion::HasShape(5));
        assert_eq!(first, again);
        assert_ne!(first, other);
        assert_eq!(domain.asserted(first), Some(JsAssertion::HasShape(4)));
    }

    /// The reason the effect takes the operand types: it is what a guard is for.
    #[test]
    fn an_addition_of_proven_numbers_calls_nothing() {
        let domain = Js::new();
        let add = domain.prim(JsPrim::Add);
        let proven = domain.effect_of(add, &[Type::Int32, Type::Int32]);
        assert!(proven.is_pure());

        let unknown = domain.effect_of(add, &[Type::Anything, Type::Int32]);
        assert!(unknown.has(Effect::CALLS_USER));
        assert!(unknown.has(Effect::THROWS));
        assert!(!unknown.falls_through());

        // And concatenation allocates even though nothing coerces.
        let text = domain.effect_of(add, &[Type::Str, Type::Str]);
        assert!(text.has(Effect::ALLOCATES));
        assert!(text.may_collect());
    }

    #[test]
    fn a_shaped_field_read_reads_and_an_unshaped_one_may_run_a_getter() {
        let domain = Js::new();
        let read = domain.prim(JsPrim::FieldRead);
        let shaped = domain.effect_of(read, &[Type::Shaped(1)]);
        assert!(shaped.has(Effect::READS));
        assert!(!shaped.has(Effect::CALLS_USER));

        let unshaped = domain.effect_of(read, &[Type::Object]);
        assert!(unshaped.has(Effect::CALLS_USER));
    }

    /// Rule 4: the index is opaque to the IR, and this is the only file that may
    /// turn one into a meaning.
    #[test]
    fn every_primitive_round_trips_through_its_index() {
        let domain = Js::new();
        for which in Js::PRIMS {
            assert_eq!(domain.meaning(domain.prim(*which)), Some(*which));
        }
        assert_eq!(domain.meaning(Prim(Js::PRIMS.len() as u32)), None);
    }

    /// A `strictEquals` cannot call user code, which is what makes it the one
    /// comparison worth its own row.
    #[test]
    fn strict_equality_coerces_nothing() {
        let domain = Js::new();
        let equals = domain.prim(JsPrim::StrictEquals);
        assert!(
            domain
                .effect_of(equals, &[Type::Anything, Type::Anything])
                .is_pure()
        );
        let less = domain.prim(JsPrim::LessThan);
        assert!(
            !domain
                .effect_of(less, &[Type::Anything, Type::Anything])
                .is_pure()
        );
    }
    /// Coercing a string or a singleton is not a call, which is the arm the first
    /// version of this table got wrong.
    #[test]
    fn coercing_anything_but_an_object_calls_nothing() {
        let domain = Js::new();
        let times = domain.prim(JsPrim::Multiply);
        assert!(domain.effect_of(times, &[Type::Str, Type::Str]).is_pure());
        assert!(
            domain
                .effect_of(times, &[Type::Undefined, Type::Int32])
                .is_pure()
        );
        // An OBJECT is the one that reaches valueOf.
        assert!(
            domain
                .effect_of(times, &[Type::Shaped(1), Type::Int32])
                .has(Effect::CALLS_USER)
        );
        assert!(
            domain
                .effect_of(times, &[Type::Object, Type::Int32])
                .has(Effect::CALLS_USER)
        );
    }
}
