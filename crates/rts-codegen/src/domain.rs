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
    /// `a > b`, `a <= b`, `a >= b`.
    ///
    /// Rows of their own rather than `LessThan` with the operands swapped, which is
    /// the rewrite this table refused for three commits and the refusal was right:
    /// `a > b` coerces `a` FIRST and `b < a` coerces `b` first, so the swap changes
    /// which `valueOf` runs first and that is observable.
    ///
    /// One row for the three because they agree about everything recorded here — each
    /// coerces both operands, each answers a boolean, each calls user code only where
    /// an operand is an object. Which comparison they are is the machine lowering's
    /// question.
    Compare,
    /// `a === b`, which coerces nothing. The one comparison that cannot call
    /// user code, which is why it is a row of its own.
    StrictEquals,
    /// `a == b`, which coerces until the two are comparable.
    ///
    /// A row apart from [`JsPrim::StrictEquals`] because it is a different operation
    /// and not a laxer spelling of the same one: it may call `valueOf` where strict
    /// equality calls nothing, so the two have different effects over the same
    /// operands.
    ///
    /// `docs/codegen/entry-tax.md` part five is about exactly this operator, and the
    /// finding is worth carrying here: `x == null` ran `ToPrimitive` on the object —
    /// two `valueOf` calls per comparison where the specification calls it zero times
    /// — and answered correctly the whole time, at 180 times the cost. The
    /// specification puts the cheap arms FIRST, and a lowering that coerces before it
    /// dispatches is the natural way to lose them.
    LooseEquals,
    /// `a instanceof b`.
    ///
    /// Reads a prototype chain, and may call user code: a constructor may carry a
    /// `Symbol.hasInstance` method, which replaces the whole algorithm. So this is
    /// not a chain walk with a fast path — it is a dispatch whose ordinary case is a
    /// chain walk.
    InstanceOf,
    /// `a in b`.
    ///
    /// Reads, and may call user code through a proxy's `has` trap. Answers a boolean.
    HasProperty,
    /// Whether a value is null or undefined, and nothing else.
    ///
    /// The condition of the coalescing operator, and a row of its own because it is
    /// NOT a truth test: `0 ?? 1` is `0` where `0 || 1` is `1`. Five of the seven
    /// falsy values are not nullish, so an operator built on `Truthy` would be wrong
    /// for every one of them.
    ///
    /// Pure: comparing against the two singletons coerces nothing.
    IsNullish,
    /// `typeof a`, which answers a string and reads nothing.
    TypeOf,
    /// `!a`, which reads this language's truth rule.
    Not,
    /// `a & b`, `a | b`, `a ^ b`, `a << b`, `a >> b`.
    ///
    /// One row for the five, because they agree about everything this table records:
    /// each coerces both operands with `ToInt32`, each answers a value that fits in
    /// an `i32`, and none can reach code the program wrote once the operands are
    /// not objects. What they disagree about is which machine instruction they
    /// become, and that is the machine lowering's question rather than this table's.
    ///
    /// `>>>` is NOT here. It answers `ToUint32`, so `-1 >>> 0` is 4294967295 — a
    /// number an `i32` cannot hold, and the one bitwise operator whose answer is not
    /// an `Int32`. Giving it this row would be wrong at exactly the value that
    /// distinguishes it.
    BitwiseInt32,
    /// `-a`, which coerces and then negates.
    ///
    /// Apart from a subtraction from zero: `-0` is `-0` and `0 - 0` is `+0`, and the
    /// two are distinguishable by `Object.is` and by division. Lowering one as the
    /// other would be wrong at the value that names the difference.
    Negate,
    /// `~a`, which answers an `Int32` like the binary bitwise row.
    BitwiseNot,
    /// The receiver of this activation.
    ///
    /// # Why an operation and not a parameter
    ///
    /// The same answer the outer binding got, for the same reason. WHERE the
    /// receiver of an activation lives — an extra parameter, a register the
    /// convention reserves, a slot the frame holds — is the machine's calling
    /// convention, and this crate's rule 2 says a machine question is never decided
    /// here.
    ///
    /// It is also the other end of `rts_mir::cfg::Op::Call`'s receiver field: one
    /// says a receiver travels, this says the callee reads it, and neither packs it
    /// into an argument list. The two are refused together in `rts-mir/lower` under
    /// `NeedsReceiverConvention`, which is the honest place for the decision.
    ThisValue,
    /// Reading a name no scope declares, through the global object.
    ///
    /// # Why this is not a refusal and not an entry point either
    ///
    /// A name the whole program declares nowhere is resolved through the global
    /// object at run time — which `emit/inline.rs` already states as the reason zero
    /// declarations is a STRONGER proof than one. So it is an operation like any
    /// other read, and it takes a key: the same declared constant a property read
    /// takes, because that is what it is.
    ///
    /// Naming an entry point instead would be the wrong shape. `Math` is not a
    /// runtime operation, it is a property of an object, and `rts-host`'s entry table
    /// is for operations. The day a lowering wants `Math.abs` as one instruction, the
    /// thing that earns it is a proof that nobody reassigned `Math` — which is
    /// `primordial`'s question and a pass, not a table row.
    GlobalRead,
    /// Constructing, with the constructor as the first argument.
    ///
    /// NOT a call, and the difference is what it does rather than how it is written:
    /// it allocates an object, runs a body against it, and answers the object unless
    /// the body answered another one. `Op::Call` cannot say that — its result is
    /// whatever the callee returned — and giving construction a flag on the call
    /// would make every pass reading a call ask which kind it was.
    Construct,
    /// A function value, naming which function of the module it is.
    ///
    /// # Why this is an operation and not a refusal
    ///
    /// The third time rule 2 answers the same shape. A function value is a closure:
    /// code plus whatever its free bindings resolve to. The CODE is known — the
    /// module numbered it — and the environment is where its free bindings live,
    /// which is the machine question `OuterRead` already leaves below.
    ///
    /// So this says WHICH FUNCTION and stops. A machine lowering decides what a
    /// closure is made of, and it already has everything it needs to: the function's
    /// own graph reads its free bindings through `OuterRead`, so the set is derivable
    /// from the graph rather than something this operation has to carry.
    ///
    /// That is what makes it one argument instead of a captured list — and why a pass
    /// that wants the captured set reads the callee's graph, which is the one place
    /// the answer cannot drift from.
    MakeClosure,
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
    /// This language ToNumber, as an operation.
    ///
    /// An increment needs it: i++ answers the NUMBER the target held, not the
    /// target, so a string target answers 5 rather than the string. Coercing an
    /// object reaches valueOf and therefore user code; coercing anything else is
    /// total and pure, which is the boundary effect_of already draws.
    ToNumber,
    /// Reading a property by an index this program computed.
    IndexRead,
    /// Writing one.
    IndexWrite,
    /// Reading a binding declared outside the function being lowered.
    ///
    /// # Why this is an operation and not a parameter
    ///
    /// Closure conversion is the other answer: make every free binding an extra
    /// parameter and have each call site supply it. It is the standard move and it
    /// does not fit yet — a `Callee::Dynamic` site does not know the callee's free
    /// set, so the conversion would have to refuse exactly the calls that most need
    /// it.
    ///
    /// What this does instead is say *which binding* and stop. WHERE its cell lives
    /// — a module record, an environment object, a slot some enclosing activation
    /// holds — is a machine question, and this crate's rule 2 is that a machine
    /// question is never decided here. The front end's `MachineOps` answers it when
    /// there is one.
    ///
    /// Takes one argument: a declared constant naming the binding. So two reads of
    /// one outer binding carry one index and compare equal, which is what a pass
    /// hoisting a load out of a loop needs.
    OuterRead,
    /// Writing one.
    OuterWrite,
    /// An object built from its written properties, in order.
    ///
    /// Variadic and in PAIRS: a declared key, then its value, repeated. The pairs
    /// are in source order because that is the order the properties are added,
    /// which is what decides the layout — the tree's own comment on the node says
    /// so, and reordering them here would silently mint a different shape at run
    /// time.
    ///
    /// It does NOT carry a shape. See [`Type::Shaped`] for why that is a finding
    /// rather than an omission.
    NewObject,
    /// An array built from its elements, in order.
    ///
    /// Variadic: an element per argument. The count is the literal written, which
    /// a spread would change at run time -- so a spread is refused by the lowering
    /// rather than represented here.
    NewArray,
}

/// A constant this language declares, by the index an [`rts_mir::Const::Declared`]
/// carries.
///
/// # Why the IR carries an index and not the thing
///
/// Rule 4's other half. A property key is an interned name of this front end and a
/// string is its text; neither is something `rts-mir` could hold without knowing
/// what a name or a string is here, and the next language's answers are its own.
/// So the IR carries a number and this table says what the number means.
///
/// The first two entries are fixed and the fixing is load-bearing: `lower` writes a
/// `values::Singleton`'s discriminant straight into `Const::Declared`, and
/// `lower_tests.rs` pins the agreement. Reordering that enum would make `undefined`
/// mean `null`, which no assertion about behaviour would catch.
#[derive(Clone, PartialEq, Debug)]
pub enum JsConst {
    /// `undefined` or `null`, by `values::Singleton`'s own numbering.
    Singleton(crate::values::Singleton),
    /// A property key, as this front end interned it.
    Key(crate::names::Name),
    /// A string the program wrote, as the code units it means.
    ///
    /// Apart from [`JsConst::Key`] although both are text: a key names a position
    /// in a layout and a string is a value, and a pass folding one must not fold
    /// the other.
    Text(crate::syntax::Text),
    /// A declaration of this program, by its [`crate::names::resolve::BindingId`]
    /// index.
    ///
    /// Not a value the program can hold: it names a binding, so that an operation
    /// reading or writing one outside the function being lowered can say WHICH
    /// without this layer deciding where the binding's cell lives. See
    /// [`JsPrim::OuterRead`].
    Binding(u32),
    /// A function of this program, by its `rts_mir::cfg::FuncId` index.
    ///
    /// Beside [`JsConst::Binding`] and for the same reason: it names something rather
    /// than being a value, so that an operation can say WHICH without this layer
    /// deciding how the thing is represented.
    Function(u32),
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
    /// What each declared-constant index means. The first two are the singletons,
    /// put there by [`Js::new`] so the numbering the lowering writes is the one
    /// this reads.
    constants: Vec<JsConst>,
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
        JsPrim::ToNumber,
        JsPrim::IndexRead,
        JsPrim::IndexWrite,
        JsPrim::NewArray,
        JsPrim::NewObject,
        JsPrim::OuterRead,
        JsPrim::OuterWrite,
        JsPrim::BitwiseInt32,
        JsPrim::Negate,
        JsPrim::BitwiseNot,
        JsPrim::ThisValue,
        JsPrim::GlobalRead,
        JsPrim::Construct,
        JsPrim::MakeClosure,
        JsPrim::Compare,
        JsPrim::IsNullish,
        JsPrim::LooseEquals,
        JsPrim::InstanceOf,
        JsPrim::HasProperty,
    ];

    /// A domain holding only the fixed constants.
    pub fn new() -> Self {
        Self {
            assertions: Vec::new(),
            constants: crate::values::Singleton::ALL.iter().map(|held| JsConst::Singleton(*held)).collect(),
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
            | JsPrim::LessThan
            | JsPrim::Compare
            | JsPrim::LooseEquals
            | JsPrim::BitwiseInt32
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
            | JsPrim::ThisValue => Effect::PURE,
            // The same boundary as arithmetic: only an object coerces through code
            // the program wrote.
            // Building one allocates, whatever it is built from, and it reaches
            // no code the program wrote: the elements are already values.
            JsPrim::NewArray | JsPrim::NewObject => Effect::ALLOCATES,
            // A READ of an outer binding loads a cell and may find it in its
            // temporal dead zone, which throws. It calls nothing: a binding is not
            // a property, so no getter is reachable through one.
            JsPrim::OuterRead => Effect::READS.and(Effect::THROWS),
            JsPrim::OuterWrite => Effect::WRITES.and(Effect::THROWS),
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
            // The language's own table, which is where a key and a string live
            // beside the two singletons.
            Const::Declared(index) => match self.declared(*index) {
                Some(JsConst::Singleton(crate::values::Singleton::Undefined)) => Type::Undefined,
                Some(JsConst::Singleton(crate::values::Singleton::Null)) => Type::Null,
                // A key is a string, and so is a string.
                Some(JsConst::Key(_) | JsConst::Text(_)) => Type::Str,
                // A binding NAME is not a value of the language, so it has no type
                // in this lattice. Nothing reads one as a value: it is only ever an
                // operand of an outer read or write.
                Some(JsConst::Binding(_) | JsConst::Function(_)) => Type::Nothing,
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
            // ALWAYS an Int32, whatever it was given, and that is the reason a
            // program writes one: `x | 0` is how a number becomes provably narrow.
            JsPrim::BitwiseInt32 | JsPrim::BitwiseNot => Type::Int32,
            // A negation answers a number, and never an Int32: negating the most
            // negative one does not fit, and negative zero is not a value this
            // lattice can name apart from zero.
            JsPrim::Negate => Type::Double,
            // Nothing is known about a receiver without a proof about the call site.
            JsPrim::ThisValue => Type::Anything,
            JsPrim::LessThan
            | JsPrim::Compare
            | JsPrim::StrictEquals
            | JsPrim::Not
            | JsPrim::IsNullish
            | JsPrim::LooseEquals
            | JsPrim::InstanceOf
            | JsPrim::HasProperty => {
                Type::Bool(None)
            }
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
            JsPrim::FieldRead | JsPrim::IndexRead | JsPrim::OuterRead | JsPrim::GlobalRead => {
                Type::Anything
            }
            // An object, and never a shaped one: which layout a constructor arrives
            // at is the runtime shape tree's answer. See [`Type::Shaped`].
            JsPrim::Construct => Type::Object,
            JsPrim::MakeClosure => Type::Callable,
            JsPrim::OuterWrite => args.get(1).cloned().unwrap_or(Type::Anything),
            JsPrim::IndexWrite => args.get(2).cloned().unwrap_or(Type::Anything),
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
        // The entry table is `rts-host`'s and this crate does not hold it yet, so
        // the honest answer is the number. Naming it wrongly would be worse than
        // not naming it: a reader would trust it.
        format!("entry#{}", entry.0)
    }

    fn declared(&self, index: u32) -> String {
        match Js::declared(self, index) {
            Some(JsConst::Singleton(which)) => format!("{which:?}").to_lowercase(),
            Some(JsConst::Key(_)) => format!("key#{index}"),
            // A binding, printed as its index. The NAME lives in the interner, so
            // this crate can honestly say only which declaration it is --
            // `mir_dump::Spelled` is what turns it into a name.
            Some(JsConst::Binding(held)) => format!("binding#{held}"),
            Some(JsConst::Function(held)) => format!("f{held}"),
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
