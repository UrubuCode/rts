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
    /// `a > b`, `a <= b`, `a >= b` -- one row each.
    ///
    /// Rows of their own rather than `LessThan` with the operands swapped, which is
    /// the rewrite this table refused for three commits and the refusal was right:
    /// `a > b` coerces `a` FIRST and `b < a` coerces `b` first, so the swap changes
    /// which `valueOf` runs first and that is observable.
    ///
    /// They were ONE row, `Compare`, on the reasoning that they agree about everything
    /// recorded here and "which comparison they are is the machine lowering's
    /// question". The machine lowering could not answer it: nothing in the graph said
    /// which of the three a `Compare` was, so every one of them was refused at the
    /// boundary -- 79 functions of the corpus, every one over two PROVED numbers. A question the
    /// graph does not carry cannot be answered below it.
    GreaterThan,
    /// `a <= b`.
    LessOrEqual,
    /// `a >= b`.
    GreaterOrEqual,
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
    /// WHERE the receiver of an activation lives — an extra parameter, a register a
    /// convention reserves, a slot the frame holds — is a CONVENTION, and this note used
    /// to say it was the machine's. It is not, and the correction is worth the space:
    /// `rts_cranelift::abi::Convention` is about linkage and tail calls and reserves
    /// nothing for a receiver, so the machine layer has no answer to give and asking it
    /// would have got one invented.
    ///
    /// It is this language's, and `emit/function.rs` fixes it: parameter 0 is the
    /// environment and `THIS_PARAM` is 1. `rts_core::entry::functions::invoke` calls on
    /// those terms, so the convention is an agreement between a signature and the runtime
    /// — which is why the boundary is TOLD the receiver by whoever declared the signature
    /// rather than looking for it.
    ///
    /// Still an operation and not a parameter of this graph, for the reason the outer
    /// binding is one: a lowering that made it a parameter would be choosing the position,
    /// and the position is the caller's to state.
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
    /// A function value: which function of the module it is, and the environment it
    /// closes over.
    ///
    /// # Two operands, where there used to be one
    ///
    /// This took the function alone and said the environment was "the machine
    /// question `OuterRead` already leaves below". It was not the machine's -- see
    /// [`Self::EnvRead`] -- and leaving it out of the graph meant a closure's
    /// environment was nowhere a pass or a boundary could see. So the environment
    /// in force where the closure is made is the second operand: this activation's
    /// own when it builds one, the one it was made in otherwise, `undefined` when
    /// nothing inside reaches past itself -- which is what `emit/function.rs` hands
    /// such a closure too.
    ///
    /// The captured SET is still not carried, and that part of the old reasoning
    /// holds: the callee's own graph says what it reads, which is the one place the
    /// answer cannot drift from.
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
    /// The environment this activation was created in: the one its closure carried.
    ///
    /// # Why an operation and not a parameter of the graph
    ///
    /// For the reason [`Self::ThisValue`] is one. `emit/function.rs` fixes the
    /// convention -- parameter 0 is the environment -- and the boundary is TOLD the
    /// parameters by whoever declared the signature, so a lowering that made it a
    /// parameter would be choosing a position that is the caller's to state.
    ///
    /// It is `undefined` for a function made where nothing was captured, which is
    /// what `emit/function.rs` hands such a closure. Nothing reads through it then:
    /// `names::resolve` asks for it only in a function that reaches past itself.
    EnclosingEnvironment,
    /// A fresh environment: the enclosing one first, then one declared key per
    /// captured binding this activation owns, in declaration order.
    ///
    /// # The layout, and whose it is
    ///
    /// An environment is an ordinary object holding each captured binding under its
    /// spelling, plus the link to the environment the function was made in -- the
    /// shape `emit/binding.rs` gives one. This language invented that layout, which is
    /// why the operation is here and names no machine fact.
    ///
    /// Every key is DEFINED at creation, to `undefined`. A binding a closure reads
    /// before its declaration runs is then an own property holding `undefined` rather
    /// than an absent one, and an absent one would be looked up on the prototype
    /// chain: a captured `var toString` would answer `Object.prototype.toString`.
    EnvNew,
    /// The environment an environment was made inside: one link out.
    EnvOuter,
    /// A captured binding read out of an environment, by its declared key.
    ///
    /// # Why not [`Self::FieldRead`]
    ///
    /// Because an environment is not an object the program can reach, and that is
    /// what the effect rests on: every key was defined at creation as an own data
    /// property, so no getter and no proxy trap is reachable through one. A field read
    /// promises nothing of the sort, and sharing it would make a pass read an
    /// environment as a call into user code.
    ///
    /// This replaces an operation that named the BINDING and left where it lived to
    /// "the machine" -- which had no answer to give, since `rts_cranelift` knows no
    /// environment. The distance is now a fact of the scope tree,
    /// `Resolution::hops`, and the graph says it as that many [`Self::EnvOuter`].
    ///
    /// Not a dead-zone check: a `let` read by a closure before its declaration runs
    /// answers `undefined` here where the language throws. Stated, not hidden -- it is
    /// the gap `JsPrim::EnvNew`'s definition at creation trades for never reading the
    /// prototype chain.
    EnvRead,
    /// A captured binding written, by its declared key. Answers the value written.
    EnvWrite,
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

impl JsPrim {
    /// Whether this operation turns its operands into numbers whatever they were.
    ///
    /// # Why this is a method here and not a list at each site
    ///
    /// Because it was a list at two sites, and the doc comment on the second one said so
    /// while leaving it: `lower/push.rs` matched on these rows and `lower/numeric_use.rs`
    /// matched on the `BinaryOp` spellings of them. Rule 3 -- a rule written twice is a
    /// rule that will be written differently -- with the second writing already done.
    ///
    /// They had in fact already drifted. `Negate` is here and the syntax side also
    /// collected unary `+`, whose row is [`JsPrim::ToNumber`] and which was NOT here -- so
    /// one side earned a parameter its guard and the other never speculated on the
    /// operation that motivated it. Harmless, because the guard left the operand proved
    /// either way, and invisible for exactly that reason.
    ///
    /// # What the test is, and why it is not "usually numeric"
    ///
    /// The specification coerces here. A number is then the case the operation is FOR
    /// rather than a guess about the program, which is what makes a guard on its operand
    /// speculation rather than a wager.
    ///
    /// [`JsPrim::Add`] is deliberately absent: over anything but two numbers it may
    /// concatenate, so a number is a guess there and a wrong one for every string a
    /// program holds. [`JsPrim::StrictEquals`] is absent too -- it coerces nothing, and
    /// `a === b` over two strings is ordinary code that would fall on every comparison.
    pub fn coerces_to_number(self) -> bool {
        matches!(
            self,
            JsPrim::Subtract
                | JsPrim::Multiply
                | JsPrim::Divide
                | JsPrim::Remainder
                | JsPrim::LessThan
                | JsPrim::GreaterThan
                | JsPrim::LessOrEqual
                | JsPrim::GreaterOrEqual
                | JsPrim::Negate
                | JsPrim::ToNumber
        )
    }
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
    /// It names something rather than being a value, so that an operation can say
    /// WHICH without this layer deciding how the thing is represented.
    Function(u32),
    /// A count the compiler fixed and an entry point takes as a machine word -- how many
    /// parameters a function declares, where `RestArguments` starts gathering.
    ///
    /// Not a number of the language: nothing reads one as a value, and a `Const::Int`
    /// would be one, typed and boxed as the program's numbers are.
    Count(u32),
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

impl WellKnown {
    /// The key text the runtime stores this under.
    ///
    /// One place for the six spellings, because the runtime reaches the same property by
    /// the same text and a second copy is how one comes to be `"@iterator"`. A SYMBOL is
    /// an interned name in a space no program can write -- `"@@iterator"`, which is
    /// `rts_core::entry::symbol`'s design and `emit/delegate.rs`'s spelling -- so storage,
    /// shapes and the inline cache treat it as any other key and only enumeration knows.
    pub fn spelled(self) -> &'static str {
        match self {
            WellKnown::Prototype => "prototype",
            WellKnown::IteratorSymbol => "@@iterator",
            WellKnown::Next => "next",
            WellKnown::Done => "done",
            WellKnown::Element => "value",
            WellKnown::Return => "return",
        }
    }
}

// THE ENTRY TABLE WAS A SECOND TABLE OF ONE THING, and `JsEntry` is gone rather than
// bridged. Every row it had -- `RegexNew`, `ArrayAppend`, `ArrayAppendAll` -- already
// existed in `crate::runtime::RuntimeOp`, which is this crate's one catalogue of
// operations the language calls instead of emitting.
//
// The `reuse-check` skill calls that shape fatal and says why: two tables that must
// agree about a number are two shape trees one level up. This one would have had to
// agree about a symbol name, an ABI signature AND an address, and the address is the
// one nobody could have checked -- `rts-host::entries::resolve` answers it from
// `RuntimeOp`, so a `JsEntry` row would have reached no address at all.
//
// So `rts_mir::EntryId` now indexes `Js::ENTRIES`, which is a list of `RuntimeOp`.
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
