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

pub use tables::{JsAssertion, JsConst, JsPrim, WellKnown};

use crate::runtime::RuntimeOp;

mod tables;

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
    /// A bigint -- the second numeric tower, known where a literal wrote one.
    ///
    /// Its own row and not `Anything`, for one reason a measurement gave: an operand
    /// that is a bigint is never a double, so `1n + i` asks nothing of `i` -- the
    /// running engine does not guard it, and `literal_guard_gate.rs` counts. It joins
    /// nothing but itself, and no operator of this lattice proves one: arithmetic over
    /// two of them is the runtime's, and answers whatever the runtime answers.
    BigInt,
    /// An object whose layout is known, by the machine's shape id.
    ///
    /// # Unreachable today, and that is the finding rather than an omission
    ///
    /// `reuse-check`, run 2026-09-20 before writing the object literal: the machine
    /// already owns shapes — `rts_cranelift::shape::ShapeTree` has `transition`,
    /// `slot_of` and `layout`, and `names::Names` already mints its property keys
    /// from the same `KeyRegistry`. So nothing new was written.
    ///
    /// But the `ShapeTree` that decides layouts **lives in `rts-core`'s `Context`**:
    /// it is a RUN-TIME structure, and a shape is minted as a program adds
    /// properties. The compiler holds none. So an object literal cannot answer
    /// "this is shape 7" without the compiler and the runtime agreeing about a
    /// number only one of them mints — which this crate's rule 1 names as the exact
    /// failure it was written for, *"a second shape tree disagreeing with the
    /// compiler's about which slot is which property"*, and rule 2 forbids by saying
    /// that where a field sits is never decided here.
    ///
    /// What would make it reachable is the pattern the keys already use:
    /// `Names::keyed_texts` exists so the host can install the compiler's keys into
    /// the runtime. Shapes minted at compile time and installed the same way would
    /// give both sides one numbering. That is a design change across three crates
    /// rather than a lowering, and it is the precondition for the first guard —
    /// `docs/engine/deopt-lateral.md` D1 has nothing to assert about until it
    /// exists.
    Shaped(u32),

    /// An object of unknown layout.
    Object,
    /// Something callable.
    Callable,
    /// Nothing is known.
    Anything,
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
    /// What each declared-constant index means. The first two are the singletons,
    /// put there by [`Js::new`] so the numbering the lowering writes is the one
    /// this reads.
    constants: Vec<JsConst>,
}

impl Js {
    /// Every entry point, in the order their [`EntryId`] indices run.
    ///
    /// A static table for the reason `PRIMS` is one: an entry is a row of source code
    /// here, not something a program registers.
    const ENTRIES: &'static [RuntimeOp] = &[
        RuntimeOp::RegexNew,
        RuntimeOp::ArrayAppend,
        RuntimeOp::ArrayAppendAll,
        // THE VALUE A TEXT CONSTANT IS, and the row that makes a declared constant
        // reachable at all: `runtime::raising::CANNOT_RAISE` holds it, so it is the first
        // entry point this boundary can actually emit -- every other row it named raises,
        // and a call that raises is refused for want of a branch-and-reraise.
        RuntimeOp::StringConst,
        // ToString, which a template substitution applies -- see `lower/template.rs` for
        // why it is not the `+` that joins the pieces afterwards.
        RuntimeOp::StringOf,
        // The arguments past the ones a function declares, or all of them: what a rest
        // parameter holds and where a fifth parameter is read from -- the running
        // emitter's `bind_parameters` reads both out of it.
        RuntimeOp::RestArguments,
        // The `arguments` object, from the four slots -- `emit/function.rs` makes the
        // same call where a body mentions the name.
        RuntimeOp::ArgumentsObject,
        // `yield*`: one step of the inner iterator, recorded as the one this generator
        // stands in front of; a source with no protocol, materialised; and its length.
        // `lower/delegate.rs`, after `emit/delegate.rs`.
        RuntimeOp::DelegateStep,
        RuntimeOp::Iterate,
        RuntimeOp::ArrayLength,
        // A bigint literal, from its digits -- `lower/push.rs`.
        RuntimeOp::BigIntNew,
        // A write to a name no scope declares -- `lower::Lowering::bind`, under the door.
        RuntimeOp::GlobalSet,
        // `delete`, a spread call and a spread or long construction -- `lower/places.rs`
        // and `lower/calls.rs`.
        RuntimeOp::DeleteProperty,
        RuntimeOp::CallWithArgs,
        RuntimeOp::ConstructWithArgs,
        // A NON-STRICT body's entry: the receiver substituted, and `arguments.callee`
        // defined on the arguments object -- `lower/gather.rs`, after
        // `emit/nonstrict.rs`.
        RuntimeOp::SloppyThis,
        RuntimeOp::RunningFunction,
        RuntimeOp::DefineMethod,
        // `new.target` in a function's own activation -- `lower::Lowering::expression`.
        RuntimeOp::NewTarget,
        // A tagged template's strings object, by site -- `lower::Lowering::expression`.
        RuntimeOp::TemplateStrings,
        // `for`-`in`: the keys, snapshotted, and whether one is still reachable --
        // `lower/enumerate.rs`, after `emit/foreach.rs`.
        RuntimeOp::EnumerateKeys,
        RuntimeOp::ForInHas,
        // `super(...)` in an arrow -- `lower/class.rs::super_call_in_arrow`.
        RuntimeOp::SuperConstruct,
        RuntimeOp::SuperConstructWithArgs,
        // `Math.random()` where `Math` is the language's -- `lower/intrinsic.rs`.
        RuntimeOp::MathRandom,
        // An element of an array a `for`-`of` walks by index -- `lower/iterate.rs`.
        RuntimeOp::ElementAt,
        // Whether an array may be read by index where its iterator would step it --
        // `lower/iterate.rs` and `lower/destructure.rs`.
        RuntimeOp::ArrayPatternDirect,
    ];

    /// The index the IR carries for an entry point.
    pub fn entry_point(&self, which: RuntimeOp) -> EntryId {
        let at = Self::ENTRIES
            .iter()
            .position(|held| *held == which)
            .expect("every entry named is a row of ENTRIES");
        EntryId(at as u32)
    }

    /// What an [`EntryId`] index means.
    pub fn entry_meaning(&self, entry: EntryId) -> Option<RuntimeOp> {
        Self::ENTRIES.get(entry.0 as usize).copied()
    }

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
        JsPrim::ToNumber,
        JsPrim::IndexRead,
        JsPrim::IndexWrite,
        JsPrim::NewArray,
        JsPrim::NewObject,
        JsPrim::EnvRead,
        JsPrim::EnvWrite,
        JsPrim::BitAnd,
        JsPrim::Negate,
        JsPrim::BitwiseNot,
        JsPrim::ThisValue,
        JsPrim::GlobalRead,
        JsPrim::Construct,
        JsPrim::MakeClosure,
        JsPrim::GreaterThan,
        JsPrim::LessOrEqual,
        JsPrim::GreaterOrEqual,
        JsPrim::IsNullish,
        JsPrim::LooseEquals,
        JsPrim::InstanceOf,
        JsPrim::HasProperty,
        JsPrim::EnclosingEnvironment,
        JsPrim::EnvNew,
        JsPrim::EnvOuter,
        JsPrim::Exponent,
        JsPrim::BitOr,
        JsPrim::BitXor,
        JsPrim::ShiftLeft,
        JsPrim::ShiftRight,
        JsPrim::ShiftRightUnsigned,
        JsPrim::MathSqrt,
        JsPrim::MathFloor,
        JsPrim::MathCeil,
        JsPrim::MathTrunc,
        JsPrim::MathAbs,
    ];

    /// A domain holding only the fixed constants.
    pub fn new() -> Self {
        Self {
            assertions: Vec::new(),
            constants: crate::values::Singleton::ALL
                .iter()
                .map(|held| JsConst::Singleton(*held))
                .collect(),
        }
    }

    /// Registers a constant, answering the index the IR carries.
    ///
    /// Interned, so that two reads of the same property carry the same index -- a
    /// pass that wants to know whether two accesses touch one field compares the
    /// numbers rather than the names.
    pub fn constant(&mut self, what: JsConst) -> u32 {
        match self.constants.iter().position(|held| *held == what) {
            Some(at) => at as u32,
            None => {
                self.constants.push(what);
                self.constants.len() as u32 - 1
            }
        }
    }

    /// What a declared-constant index means.
    pub fn declared(&self, index: u32) -> Option<&JsConst> {
        self.constants.get(index as usize)
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
    fn effect_of_inner(&self, prim: Prim, args: &[Type]) -> Effect {
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
            | JsPrim::Exponent
            | JsPrim::LessThan
            | JsPrim::GreaterThan
            | JsPrim::LessOrEqual
            | JsPrim::GreaterOrEqual
            | JsPrim::LooseEquals
            | JsPrim::BitAnd
            | JsPrim::BitOr
            | JsPrim::BitXor
            | JsPrim::ShiftLeft
            | JsPrim::ShiftRight
            | JsPrim::ShiftRightUnsigned
            | JsPrim::Negate
            | JsPrim::BitwiseNot => match args.iter().all(Self::needs_no_coercion) {
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
            JsPrim::StrictEquals
            | JsPrim::TypeOf
            | JsPrim::Not
            | JsPrim::Truthy
            | JsPrim::IsNullish
            // Reading the receiver reads a slot the convention decided. It cannot
            // fail and it cannot call anything: the value is already there.
            | JsPrim::ThisValue
            // A float instruction over a number the lowering already converted.
            | JsPrim::MathSqrt
            | JsPrim::MathFloor
            | JsPrim::MathCeil
            | JsPrim::MathTrunc
            | JsPrim::MathAbs => Effect::PURE,
            // The same boundary as arithmetic: only an object coerces through code
            // the program wrote.
            // Building one allocates, whatever it is built from, and it reaches
            // no code the program wrote: the elements are already values.
            JsPrim::NewArray | JsPrim::NewObject => Effect::ALLOCATES,
            // AN ENVIRONMENT IS NOT AN OBJECT THE PROGRAM CAN REACH, and every key in
            // one was defined as an own data property when it was made -- so a read
            // or a write reaches no getter, no setter and no trap, and raises nothing.
            // It reads or writes the heap and that is all. No dead-zone check either:
            // `JsPrim::EnvRead` states that gap where it is decided.
            JsPrim::EnvRead | JsPrim::EnvOuter => Effect::READS,
            JsPrim::EnvWrite => Effect::WRITES,
            JsPrim::EnvNew => Effect::ALLOCATES,
            // A parameter the convention placed, read like the receiver is.
            JsPrim::EnclosingEnvironment => Effect::PURE,
            JsPrim::ToNumber => match args.first().is_some_and(Self::needs_no_coercion) {
                true => Effect::PURE,
                false => Effect::CALLS_USER.and(Effect::THROWS),
            },
            // A shaped read is a load at a known offset; an unshaped one goes
            // through the runtime, which may run a getter.
            JsPrim::FieldRead => match args.first() {
                Some(Type::Shaped(_)) => Effect::READS,
                _ => Effect::READS.and(Effect::CALLS_USER).and(Effect::THROWS),
            },
            // An INDEXED access cannot be a known offset: the key is a value, so
            // even a shaped receiver needs the runtime to turn it into a position.
            // A pass that proves the index constant is what turns one of these into
            // a FieldRead, and it is a pass rather than something the lowering can
            // see.
            JsPrim::IndexRead => Effect::READS.and(Effect::CALLS_USER).and(Effect::THROWS),
            // Both read the heap and both may reach code the program wrote:
            // `instanceof` through Symbol.hasInstance, `in` through a proxy trap.
            JsPrim::InstanceOf | JsPrim::HasProperty => {
                Effect::READS.and(Effect::CALLS_USER).and(Effect::THROWS)
            }
            // A GLOBAL read may run a getter -- the global object is an ordinary
            // object -- and throws where the name is declared nowhere at all.
            JsPrim::GlobalRead => Effect::READS.and(Effect::CALLS_USER).and(Effect::THROWS),
            // Constructing allocates, runs a body that is user code, and throws
            // where the callee is not a constructor.
            // Building a closure allocates and nothing else: the code is already
            // compiled and the environment is read, not run.
            JsPrim::MakeClosure => Effect::ALLOCATES,
            JsPrim::Construct => Effect::ALLOCATES
                .and(Effect::CALLS_USER)
                .and(Effect::THROWS)
                .and(Effect::WRITES),
            JsPrim::IndexWrite => Effect::WRITES.and(Effect::CALLS_USER).and(Effect::THROWS),
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
            Type::Int32 | Type::Double | Type::Bool(_) | Type::Str | Type::Undefined | Type::Null
        )
    }

    /// Whether every value of this type is a number.
    fn is_numeric(of: &Type) -> bool {
        matches!(of, Type::Int32 | Type::Double)
    }

    /// Whether ToNumeric of a value of this type is certainly a Number and never a
    /// BigInt.
    ///
    /// Every primitive but a BigInt converts to a Number; an object may convert to
    /// either, through a `valueOf` the program wrote. `Nothing` is here because no
    /// value has it, so it constrains nothing.
    fn numeric_result(of: &Type) -> bool {
        matches!(
            of,
            Type::Nothing
                | Type::Undefined
                | Type::Null
                | Type::Bool(_)
                | Type::Int32
                | Type::Double
                | Type::Str
        )
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
            // The language's own table, which is where a key and a string live
            // beside the two singletons.
            Const::Declared(index) => match self.declared(*index) {
                Some(JsConst::Singleton(crate::values::Singleton::Undefined)) => Type::Undefined,
                Some(JsConst::Singleton(crate::values::Singleton::Null)) => Type::Null,
                // A key is a string, and so is a string.
                Some(JsConst::Key(_) | JsConst::Text(_)) => Type::Str,
                // A function NAME is not a value of the language, so it has no type
                // in this lattice. Nothing reads one as a value: it is only ever the
                // operand of the operation that makes a closure of it.
                Some(JsConst::Function(_) | JsConst::Count(_)) => Type::Nothing,
                // Handed to an append and nothing else, so nothing is known of it.
                Some(JsConst::Hole) => Type::Anything,
                // A key is text, wherever the key came from.
                Some(JsConst::WellKnown(_)) => Type::Str,
                None => Type::Anything,
            },
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
            // A NUMBER, UNLESS BOTH OPERANDS MAY BE A BIGINT. Every arithmetic operator
            // but `+` applies ToNumeric, not ToNumber, and a BigInt passes through it:
            // `10n - 1n` is `9n`. This row answered `Double` whatever the operands
            // were, which was sound for every value but that one -- and nothing noticed
            // while the machine refused the operation over unproved operands anyway.
            //
            // One side is enough to rule it out, because mixing the two kinds THROWS:
            // `x - 1` answers a number or raises, whatever `x` is. So `x | 0` keeps
            // proving an Int32, which is the reason a program writes one.
            JsPrim::Subtract
            | JsPrim::Multiply
            | JsPrim::Remainder
            | JsPrim::Divide
            | JsPrim::Exponent => {
                match args {
                    [one, two] if Self::numeric_result(one) || Self::numeric_result(two) => {
                        Type::Double
                    }
                    _ => Type::Anything,
                }
            }
            // ALWAYS an Int32 when either side rules a BigInt out, and that is the
            // reason a program writes one: `x | 0` is how a number becomes provably
            // narrow. `a | b` over two unknowns may be `3n | 4n`.
            JsPrim::BitAnd
            | JsPrim::BitOr
            | JsPrim::BitXor
            | JsPrim::ShiftLeft
            | JsPrim::ShiftRight => match args {
                [one, two] if Self::numeric_result(one) || Self::numeric_result(two) => {
                    Type::Int32
                }
                _ => Type::Anything,
            },
            // `>>>` answers `ToUint32`, a number and never an `Int32` -- the reason it
            // is a row of its own.
            JsPrim::ShiftRightUnsigned => match args {
                [one, two] if Self::numeric_result(one) || Self::numeric_result(two) => {
                    Type::Double
                }
                _ => Type::Anything,
            },
            JsPrim::BitwiseNot => match args {
                [one] if Self::numeric_result(one) => Type::Int32,
                _ => Type::Anything,
            },
            // A negation answers a number, and never an Int32: negating the most
            // negative one does not fit, and negative zero is not a value this
            // lattice can name apart from zero. Unless the operand may be a BigInt,
            // where `-1n` is `-1n`.
            // A double, whatever the number was: `Math.floor(2.5)` is 2 and still
            // a double here, as `Math.abs(-0)` is `+0`.
            JsPrim::MathSqrt
            | JsPrim::MathFloor
            | JsPrim::MathCeil
            | JsPrim::MathTrunc
            | JsPrim::MathAbs => Type::Double,
            JsPrim::Negate => match args {
                [one] if Self::numeric_result(one) => Type::Double,
                _ => Type::Anything,
            },
            // Nothing is known about a receiver without a proof about the call site.
            JsPrim::ThisValue => Type::Anything,
            JsPrim::LessThan
            | JsPrim::GreaterThan
            | JsPrim::LessOrEqual
            | JsPrim::GreaterOrEqual
            | JsPrim::StrictEquals
            | JsPrim::Not
            | JsPrim::IsNullish
            | JsPrim::LooseEquals
            | JsPrim::InstanceOf
            | JsPrim::HasProperty => Type::Bool(None),
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
            // An int32 stays one -- coercing a number answers the same number, so
            // the proof survives, which is what lets an increment of a proven
            // counter stay machine arithmetic.
            JsPrim::ToNumber => match args.first() {
                Some(Type::Int32) => Type::Int32,
                _ => Type::Double,
            },
            // An object of no known layout. An array HAS a shape in the machine
            // sense, and saying which one is what the shape registry answers --
            // this domain does not hold one yet, so the honest answer is the
            // weaker type rather than a number invented here.
            JsPrim::NewArray | JsPrim::NewObject => Type::Object,
            JsPrim::FieldRead | JsPrim::IndexRead | JsPrim::EnvRead | JsPrim::GlobalRead => {
                Type::Anything
            }
            // An environment is an object; the one this activation was made in may be
            // `undefined`, for a closure made where nothing was captured.
            JsPrim::EnvNew | JsPrim::EnvOuter => Type::Object,
            JsPrim::EnclosingEnvironment => Type::Anything,
            // An object, and never a shaped one: which layout a constructor arrives
            // at is the runtime shape tree's answer. See [`Type::Shaped`].
            JsPrim::Construct => Type::Object,
            JsPrim::MakeClosure => Type::Callable,
            JsPrim::EnvWrite => args.get(2).cloned().unwrap_or(Type::Anything),
            JsPrim::IndexWrite => args.get(2).cloned().unwrap_or(Type::Anything),
            // A write answers the value written, which is what makes `a = b = 1`
            // work. The operands are the receiver, the key and the value, so the value is
            // the THIRD. This read the second -- the key, a string -- so `(o.x = 5) + 1`
            // was typed as a concatenation.
            JsPrim::FieldWrite => args.get(2).cloned().unwrap_or(Type::Anything),
        }
    }

    fn of_entry(&self, entry: EntryId) -> Type {
        match self.entry_meaning(entry) {
            // A regular expression is an object, and not a shaped one: which layout it
            // arrives at is the runtime shape tree's answer.
            Some(RuntimeOp::RegexNew) => Type::Object,
            // BOTH APPENDS ANSWER THE ARRAY, which is why they answer anything useful
            // at all: a caller chains them, and an append answering `undefined` would
            // make every element after the first a read of nothing.
            Some(RuntimeOp::ArrayAppend | RuntimeOp::ArrayAppendAll) => Type::Object,
            // ToString answers a string or raises -- a symbol raises -- and never answers
            // anything else, which is what lets the `+` joining a template's pieces be
            // typed a concatenation.
            Some(RuntimeOp::StringOf) => Type::Str,
            Some(RuntimeOp::BigIntNew) => Type::BigInt,
            // A truth value, and answered unboxed -- the representation is the proof.
            Some(
                RuntimeOp::DeleteProperty | RuntimeOp::ForInHas | RuntimeOp::ArrayPatternDirect,
            ) => Type::Bool(None),
            // A fresh array of the keys, which nothing else can name.
            Some(RuntimeOp::EnumerateKeys) => Type::Object,
            // A length is a number, answered unboxed -- the representation and the
            // proof are one fact here.
            Some(RuntimeOp::ArrayLength | RuntimeOp::MathRandom) => Type::Double,
            // EVERY OTHER ROW OF THE CATALOGUE answers the widest thing, and that is a
            // change of shape worth stating: the old table held only what this lowering
            // reached, so a row it did not know was unrepresentable. `RuntimeOp` holds
            // every operation the language can call, so most of them are rows this
            // lowering has not learned anything about yet -- and `Anything` is the honest
            // answer for those rather than a gap the compiler would report.
            Some(_) | None => Type::Anything,
        }
    }

    fn effect_of(&self, prim: Prim, args: &[Type]) -> Effect {
        self.effect_of_inner(prim, args)
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
            // alone: `0`, `-0`, `NaN`, `""` and `0n` are values of `Int32`,
            // `Double`, `Str` and `BigInt`. The toy domain answers `Some(true)` for all three,
            // because its language has two falsy values and a zero is true.
            Type::Int32 | Type::Double | Type::Str | Type::BigInt => None,
            // An object is truthy whatever it holds, and so is a function.
            Type::Shaped(_) | Type::Object | Type::Callable => Some(true),
            Type::Nothing | Type::Anything => None,
        }
    }
}

/// What this language calls the things `rts-mir` only numbers.
///
/// The other half of rule 4: the IR carries indices and refuses to interpret them,
/// so reading a graph needs the table that minted them — and the table is here,
/// where it is also what decides effects and transfers. One source for both, which
/// is why the printer asks rather than carrying names on its instructions.
impl rts_mir::text::Legend for Js {
    fn prim(&self, prim: rts_mir::Prim) -> String {
        match self.meaning(prim) {
            Some(which) => format!("{which:?}").to_lowercase(),
            None => format!("prim#{}", prim.0),
        }
    }

    fn assertion(&self, assertion: rts_mir::Assertion) -> String {
        match self.asserted(assertion) {
            Some(what) => format!("{what:?}"),
            None => format!("assert#{}", assertion.0),
        }
    }

    fn entry(&self, entry: rts_mir::cfg::EntryId) -> String {
        match self.entry_meaning(entry) {
            Some(which) => format!("{which:?}").to_lowercase(),
            None => format!("entry#{}", entry.0),
        }
    }

    fn declared(&self, index: u32) -> String {
        match Js::declared(self, index) {
            Some(JsConst::Singleton(which)) => format!("{which:?}").to_lowercase(),
            Some(JsConst::Key(_)) => format!("key#{index}"),
            Some(JsConst::Function(held)) => format!("f{held}"),
            Some(JsConst::Count(held)) => format!("count#{held}"),
            Some(JsConst::Hole) => "hole".to_owned(),
            Some(JsConst::WellKnown(which)) => format!(".{}", format!("{which:?}").to_lowercase()),
            Some(JsConst::Text(_)) => format!("str#{index}"),
            None => format!("const#{index}"),
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

    /// Division answers a double even of two integers -- and only a number where one
    /// side rules a BigInt out. This test asserted `Double` over two unknowns, which is
    /// what the table said and what the language does not: `10n / 3n` is `3n`.
    #[test]
    fn division_answers_a_number_unless_both_sides_may_be_a_bigint() {
        let domain = Js::new();
        let divide = domain.prim(JsPrim::Divide);
        assert_eq!(
            domain.transfer(divide, &[Type::Int32, Type::Int32]),
            Type::Double
        );
        assert_eq!(
            domain.transfer(divide, &[Type::Anything, Type::Int32]),
            Type::Double,
            "mixing a BigInt with a Number throws, so one side is enough"
        );
        assert_eq!(
            domain.transfer(divide, &[Type::Anything, Type::Anything]),
            Type::Anything
        );
    }

    /// The same for every ToNumeric row, the bitwise ones included: `x | 0` still proves
    /// an Int32 -- the reason a program writes it -- and `~x` over an unknown may be
    /// `~1n`.
    #[test]
    fn a_numeric_row_over_two_possible_bigints_proves_nothing() {
        let domain = Js::new();
        for which in [JsPrim::Subtract, JsPrim::Multiply, JsPrim::Remainder] {
            let prim = domain.prim(which);
            assert_eq!(
                domain.transfer(prim, &[Type::Object, Type::Anything]),
                Type::Anything,
                "{which:?}"
            );
            assert_eq!(
                domain.transfer(prim, &[Type::Str, Type::Anything]),
                Type::Double,
                "{which:?}"
            );
        }
        let bitwise = domain.prim(JsPrim::BitAnd);
        assert_eq!(
            domain.transfer(bitwise, &[Type::Anything, Type::Int32]),
            Type::Int32
        );
        assert_eq!(
            domain.transfer(bitwise, &[Type::Anything, Type::Anything]),
            Type::Anything
        );
        let not = domain.prim(JsPrim::BitwiseNot);
        assert_eq!(domain.transfer(not, &[Type::Anything]), Type::Anything);
        let negate = domain.prim(JsPrim::Negate);
        assert_eq!(domain.transfer(negate, &[Type::Anything]), Type::Anything);
        assert_eq!(domain.transfer(negate, &[Type::Int32]), Type::Double);
    }

    /// A field write answers the VALUE written -- the third operand -- and not the key.
    #[test]
    fn a_field_write_answers_what_was_written() {
        let domain = Js::new();
        let write = domain.prim(JsPrim::FieldWrite);
        assert_eq!(
            domain.transfer(write, &[Type::Object, Type::Str, Type::Int32]),
            Type::Int32
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
