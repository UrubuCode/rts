//! The tables an index means something through.
//!
//! Apart from the lattice because `domain.rs` passed the 1000-line ceiling, and the
//! seam is the one the file already had: a TABLE says what a number means, and the
//! lattice says what a value is. Nothing here computes over a type.
//!
//! All four are the same idea — `rts-mir` carries an opaque index and this crate says
//! what it names — and the idea is rule 4 of that crate: the IR treats the number as
//! opaque, so nothing outside these tables may depend on which number anything has,
//! including the next language's tables.

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
    /// A key the LANGUAGE fixes, rather than one the program wrote.
    ///
    /// Apart from [`JsConst::Key`] because that one holds an interned name of the
    /// program's text and this one does not come from any text: a constructor's
    /// prototype lives under a key the language decided, and asking the interner for it
    /// would need a mutable interner in the lowering for a string that is not the
    /// program's.
    WellKnown(WellKnown),
    /// A function of this program, by its `rts_mir::cfg::FuncId` index.
    ///
    /// Beside [`JsConst::Binding`] and for the same reason: it names something rather
    /// than being a value, so that an operation can say WHICH without this layer
    /// deciding how the thing is represented.
    Function(u32),
}

/// A key this language fixes.
///
/// Five now, and the four that arrived are the ones this enum's first version
/// predicted: *"a second is expected — the iterator key a `for`-`of` reads is the same
/// shape of thing"*. It reads four rather than one.
///
/// # Why these are here and not asked of the interner
///
/// Because the program never wrote them. `for (const x of xs)` contains no `next`, no
/// `done` and no `value`, so there is no spelling in the source for the interner to
/// have a `Name` for — and minting one during lowering would need a mutable interner
/// here, which is the same reason `class.rs` gives for [`Self::Prototype`].
///
/// They are KEYS and not operations: reading `done` off a step result is an ordinary
/// property read, and a `Prim` for it would be this language claiming the read is
/// special when only the key is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WellKnown {
    /// Where a constructor's prototype object lives.
    ///
    /// An ordinary own property of the function, which is why it is a key at all: the
    /// link an instance gets is a different slot and a machine question, and writing
    /// this one is what a program does when it writes `F.prototype = …`.
    Prototype,
    /// `Symbol.iterator` — what a `for`-`of` asks a source for.
    ///
    /// A SYMBOL and not a string, which is the whole point of it: an object with a
    /// property literally named "Symbol.iterator" is not iterable, and a lowering that
    /// read a string key would make it so. What a symbol key IS belongs to the
    /// runtime's key numbering, which is why this names it rather than describing it.
    IteratorSymbol,
    /// `next` — the method one step of the protocol calls.
    Next,
    /// `done` — whether the sequence ended, read off the step's result.
    ///
    /// Read with `Truthy` rather than compared to `true`, because the specification
    /// says ToBoolean: an iterator answering `done: 1` ends the loop, and one answering
    /// `done: ""` does not.
    Done,
    /// `value` — the element, read off the step's result.
    Element,
    /// `return` — what an iterator is owed when a loop leaves it early.
    ///
    /// Named here although nothing calls it yet, because the obligation is stated in
    /// the tree already: `ForEachSource::owes_iterator_close`. A key with no caller
    /// would be dead, so this one arrives with the lowering that owes it.
    Return,
}

/// An operation of the runtime this language names.
///
/// # Why a table here and not in `rts-mir`
///
/// `rts_mir::cfg::EntryId` is an opaque index for the reason a `Prim` is: what a
/// runtime offers is the language's business, and the next language's table is its own.
/// This is that table, and it is the first thing to make `Callee::Entry` reachable.
///
/// # Why an entry and not a primitive
///
/// A primitive is something the machine can be told to compute; an entry is something
/// the runtime DOES. Building a regular expression compiles a pattern, allocates an
/// object and installs its state — none of which a lowering can express as
/// instructions, and all of which one call can ask for.
///
/// `rts-host/src/entries.rs` is where the name and the ABI shape of one are agreed, and
/// this table is what a lowering names before that agreement is reached. An index that
/// nothing implements yet is honest: the graph says which operation it wants, and the
/// machine boundary refuses until the entry exists.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JsEntry {
    /// Builds a regular expression from its pattern and its flags.
    ///
    /// Two arguments, both text. The pattern is compiled once per evaluation of the
    /// literal, and the object carries mutable state — `lastIndex` — which is why two
    /// evaluations of one literal are two objects and not one shared value.
    RegexNew,
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
