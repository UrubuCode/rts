//! The machine boundary, from this language's side.
//!
//! # What this is, and what was missing before it
//!
//! `rts_mir::lower::lower` walks a graph and asks the language four questions: what
//! representation a parameter holds, what a declared constant is, what a primitive
//! computes, and what an entry point call becomes. Until this file there was **no
//! implementation of `MachineOps` for JavaScript anywhere** — the only one in the
//! workspace was a toy with one primitive, inside `rts-mir`'s own tests. So the stage
//! lowered most of the corpus's functions to a graph and no graph became code.
//!
//! This is the first producer, and it is narrow on purpose.
//!
//! # The trait had to grow for it, and the hole was exactly here
//!
//! `MachineOps::prim` received the machine values and nothing else. `param_repr` takes
//! a `ValueId` and its own doc says the answer *"comes from what a pass proved about
//! the value"* — so a front end could set a parameter's representation from the lattice
//! and then have no way to lower `-` as a machine instruction, because it could not ask
//! what the OPERANDS were proved to be.
//!
//! The type domain arrived at the boundary and was unusable there, which is a fair
//! account of why nothing had implemented the trait. It now takes the instruction.
//!
//! # What this slice lowers, and the measurement that decided it
//!
//! The first draft lowered integer arithmetic and every arm of it was dead. The lattice
//! says why: **`Subtract`, `Multiply`, `Divide` and `Remainder` answer `Double` whatever
//! their operands were**, because the result may not fit in an `i32` — which
//! `domain/tables.rs` records as a finding about `+` and is true of all four. So there
//! is no integer-domain arithmetic to emit, and a guard on `result == Int32` was a
//! condition nothing satisfies.
//!
//! What the lattice DOES prove is "this is a number", so this works in the double
//! domain: each numeric operand is brought to `F64` and the float instruction is
//! emitted. An integer operand converts, which is what `FuncBuilder::to_f64` exists
//! for, and a literal arrives as `I32` because that is the representation `rts_mir`'s
//! own constant lowering gives it.
//!
//! # The bound this hits, and it is the honest headline
//!
//! **A parameter is `Anything`.** Rule 4 of this crate's README says a type annotation
//! is evidence and not proof, so `function f(a: number)` proves nothing about `a`, and
//! every primitive over it is refused here. What would turn `Anything` into `Int32` is
//! a GUARD, and nothing in this crate emits one — `Op::Guard` has zero producers today.
//!
//! So this boundary works, and what it reports is that the type domain has nothing to
//! give it yet for any function that takes an argument. That is not a defect in the
//! boundary; it is the next piece, named.
//!
//! # Why every refusal names one thing
//!
//! "Unsupported" spread over four causes is what made the old engine's second row of
//! failures un-triageable for weeks. A refusal here says which operand, and what it was
//! proved to be instead.

use rts_cranelift::ir::{CmpOp, FuncBuilder, FuncId, FuncRegistry, NumOp, ValueId as MachineValue};
use rts_cranelift::repr::Repr;
use rts_mir::cfg::{EntryId, Inst, Op, Prim, ValueId};
use rts_mir::guard::{Assertion, PointId};
use rts_mir::infer::Types;
use rts_mir::lower::MachineOps;

use crate::domain::{Js, JsAssertion, JsConst, JsPrim, Type};

/// This language, answering the machine's four questions.
pub struct JsMachine<'a> {
    /// The tables a `Prim` index means something through.
    domain: &'a Js,
    /// What each value was proved to be, from `rts_mir::infer`.
    ///
    /// Held rather than recomputed per question: the inference is a fixed point over the
    /// whole function, and asking again per instruction is the same answer at a worse
    /// price.
    types: Types<Type>,
    /// The other tier of this same function, and the registry it was declared in.
    ///
    /// `None` is a body compiled with no fall available, which is the generic tier
    /// itself: it emits no guard, so it is never asked. A specialised body reaching a
    /// guard without one refuses by name rather than trapping -- a fall that aborted
    /// would turn a speculation that did not hold into a crashed program, which is
    /// worse than the speculation never being made.
    /// The other tier of this same function, by id.
    ///
    /// The ID ALONE, because the registry it lives in is `Shared`'s -- and holding a
    /// second reference to it was what made a boundary able to fall OR to declare a call
    /// and never both: two borrows of one registry, one shared and one mutable. The
    /// instrument then measured a function that falls without entry points and one that
    /// does not with them, which is a limit nothing but this field imposed.
    generic: Option<FuncId>,
    /// Where a runtime operation's `FuncId` comes from, and the registry it lives in.
    ///
    /// # Why the language holds this and the machine does not
    ///
    /// Because a `FuncId` is an agreement about a SYMBOL, and rule 2 of the machine's
    /// README forbids that layer knowing a language's names. So the catalogue of
    /// operations, their signatures and the lazy declaration are all this crate's --
    /// `crate::runtime::RuntimeCalls`, which the old emitter has used all along.
    ///
    /// `None` is a boundary asked to lower a call with nowhere to declare it, which
    /// refuses by name rather than inventing an id. A test wants that case.
    shared: Option<Parts<'a>>,
    /// The machine value this activation's receiver arrived in.
    ///
    /// # Why the CALLER supplies it and this does not find it
    ///
    /// Because where a receiver lives is an agreement between a function's SIGNATURE and
    /// whoever calls it, and whoever declared the signature is the only one that knows.
    /// `emit/function.rs` fixes the layout for the running engine -- parameter 0 is the
    /// environment and `THIS_PARAM` is 1 -- and `rts_core::entry::functions::invoke` calls
    /// on those terms.
    ///
    /// The domain's own note on `ThisValue` said this was the MACHINE's question. It is
    /// not: `abi::Convention` is about linkage and tail calls and reserves nothing for a
    /// receiver, so the position is a language convention and `rts-cranelift` would have
    /// no answer to give. Corrected there in the same change as this.
    receiver: Option<MachineValue>,
    /// Every parameter this activation arrived with, in the signature's order.
    ///
    /// # Why a fall needs ALL of them and not the live set
    ///
    /// Because the other tier is a function with the same signature, and entering it means
    /// handing over what this one was called with -- the convention's leading two
    /// included. `MachineOps::fall` receives the live set, which for an entry guard is the
    /// PROGRAM's parameters, and passing those alone is a call of the wrong arity.
    ///
    /// Measured rather than reasoned: passing the live set took `bench/` from 12 functions
    /// reaching the machine to 6, with `CallArity { expected: 3, found: 1 }` 287 times.
    incoming: Vec<MachineValue>,
    /// The program's interner, which holds the pairing of a name to a machine key.
    ///
    /// Mutable because a key is minted on FIRST USE: `Names::key` says why -- only names
    /// used as property keys ever get one, so a program that names a thousand variables
    /// and two properties should spend two keys.
    ///
    /// Given rather than rebuilt, and that is the whole point: a second `Name -> Key` map
    /// here would be two tables of one number, and the runtime resolves a COMPUTED key by
    /// arriving at the number the compiler chose -- so a second numbering would make
    /// `o[k]` reach a different property from `o.k`.
    names: Option<&'a mut crate::names::Names>,
    /// The block this function re-raises through, built the first time one is needed.
    ///
    /// SHARED among every check, which `emit/expr.rs` measured the cost of not doing: one
    /// copy per site was 1 069 copies of the identical three lines in `bench/analytic.ts`,
    /// 20% of every basic block in the file.
    ///
    /// Sound because the block reads NOTHING from the site that branches to it -- no
    /// parameters, and its only instruction is a call with no arguments -- so there is no
    /// value that has to dominate anything and two sites reaching one copy cannot disagree
    /// about what it computes.
    ///
    /// One per REGION, and it was one per function while this boundary refused a region:
    /// where a `Throw` lands is decided by the region its block is in, so sharing between
    /// a site inside a `try` and one outside would route the outer one into a handler that
    /// never protected it. Keyed as `BodyState::reraise_in` is, by the region of the site
    /// -- which the made block inherits, because the builder is told to let a block made
    /// while emitting join the region of the block being emitted.
    reraise: std::collections::BTreeMap<
        Option<rts_cranelift::unwind::RegionId>,
        rts_cranelift::ir::BlockId,
    >,
    /// The calls in TAIL position, by the value each answers -- see [`tail_positions`].
    tail: std::collections::BTreeSet<ValueId>,
    /// Whether this body is non-strict -- see [`JsMachine::sloppy`].
    sloppy: bool,
    /// The property reads a call takes its callee from -- see [`method_reads`].
    method_reads: std::collections::BTreeSet<ValueId>,
    /// The block this body started in, where the throw flag's address is asked for
    /// once -- see [`JsMachine::asking_once_in`].
    body_entry: Option<rts_cranelift::ir::BlockId>,
    /// That address, once asked for.
    thrown_address: Option<MachineValue>,
    /// The global reads of a name nothing placed, which raise `ReferenceError` when the
    /// name is absent where every other global read answers `undefined`. The caller
    /// decides which, because only it can see the lists a name is placed by.
    unbound: std::collections::BTreeSet<ValueId>,
    /// Whether a suspension here PARKS the frame: a generator's `yield`, a plain async
    /// function's `await`. See [`JsMachine::parking`].
    parks: bool,
    /// Whether the block being lowered is part of a cleanup -- `rts_mir::lower` says so
    /// before each block. See [`JsMachine::recheck_throw`] for what it costs.
    in_cleanup: bool,
    /// Which declared constant each machine value came from.
    ///
    /// # Why this is needed and the instruction is not enough
    ///
    /// A cached read takes the key as a `shape::Key` -- a number fixed while compiling --
    /// and the graph hands its key over as an ordinary operand. `MachineOps::prim` receives
    /// the instruction, so it knows WHICH value the key is; it does not know what defined
    /// that value, because an `Inst` carries its operands and not their definitions.
    ///
    /// So the machine value is recorded where it is made. The alternative was passing the
    /// whole graph to every question, which would let any answer depend on anything.
    from_constant: std::collections::BTreeMap<MachineValue, u32>,
}

/// The machine's own failures travel out as the machine's words.
///
/// A lowering that paraphrased the layer below it would be a second account of one fact,
/// and the two would disagree the first time either changed.
/// This value in the representation something else declares.
///
/// A free function because it never needed the boundary's state, which the borrow
/// checker is what said: an entry-point call coerces its arguments while holding the
/// declaration table, and `&mut self` made the two exclusive for no reason at all.
fn coerced(
    into: &mut FuncBuilder,
    value: MachineValue,
    want: Repr,
) -> Result<MachineValue, String> {
    let found = into.repr_of(value);
    match (found, want) {
        // AN INTEGER JOINING A DOUBLE, which is the case this exists for: `let x = n;
        // if (c) { x = 2; }` joins a guarded double with a literal, the lattice proves
        // `Double`, and the literal arrives as an integer. Every value an `i32` holds
        // is a double exactly, so nothing is lost and no check is needed.
        (Repr::I32, Repr::F64) => into.to_f64(value).map_err(machine),
        // AN INTEGER INTO THE GENERIC FORM goes through the double first, so a number
        // leaves this stage in the one encoding the running engine gives every literal.
        // The machine would box it under `TAG_INT32` -- a legal word, and one every
        // native that reads `as_f64` rather than `numeric` answers `NaN` or "not a
        // number" for. The integer tag is a proof the running engine hands out at
        // named places, and a literal in a function this stage took is not one of them.
        (Repr::I32, Repr::Tagged) => {
            let double = into.to_f64(value).map_err(machine)?;
            Ok(into.widen(double))
        }
        // ANYTHING ELSE INTO THE GENERIC FORM is always available: widening is what the
        // tagged representation is for.
        (_, Repr::Tagged) => Ok(into.widen(value)),
        // AN INTEGER INTO THE WIDER INTEGER is sound -- every `i32` is an `i64` -- and
        // the machine has no instruction for it. Refused with that as the reason rather
        // than as a narrowing, which is what this arm answered before and is the wrong
        // thing to tell a reader: one of those is a missing capability and the other is a
        // check nobody wrote.
        (Repr::I32, Repr::I64) => Err(
            "an integer into the wider integer is sound and the machine has no instruction for it"
                .to_owned(),
        ),
        // AND THE OTHER DIRECTION IS REFUSED, which is the half worth stating. Going
        // from a double to an integer loses values, and going from the generic form to
        // anything is a NARROWING -- the machine's rule 11 says narrowing is never
        // automatic and a tagged value passes a guard first. A coercion that did it
        // here would be a guard nobody wrote and nobody checks.
        _ => Err(format!(
            "{found:?} into {want:?} is a narrowing, which needs a guard rather than a coercion"
        )),
    }
}

/// A value the lattice PROVED a number and the machine holds tagged, in the double domain.
///
/// Where it comes from: an operator the lattice answers `Double` for -- `x * 2` rules a
/// BigInt out, so it is a number whatever `x` is -- lowered as a guard with the runtime's
/// call on the failing edge, whose answer is a tagged word. The proof is sound and the
/// representation did not follow it.
///
/// A number has TWO encodings, and both are read: a double, and an integer under
/// `TAG_INT32`, which is the second encoding `rts-core` holds. A word that is neither
/// means the lattice was wrong, and it TRAPS rather than reading garbage as a double --
/// a crash names the defect where a wrong number would not.
pub(super) fn unbox_number(into: &mut FuncBuilder, held: MachineValue) -> Result<MachineValue, String> {
    let join = into.create_block();
    let result = into.add_block_param(join, Repr::F64);
    let double = into.create_block();
    let as_double = into.add_block_param(double, Repr::F64);
    let not_double = into.create_block();
    into.guard(held, Repr::F64, (double, &[]), (not_double, &[]))
        .map_err(machine)?;
    into.switch_to(double);
    into.jump(join, &[as_double]).map_err(machine)?;

    into.switch_to(not_double);
    let integer = into.create_block();
    let as_integer = into.add_block_param(integer, Repr::I32);
    let neither = into.create_block();
    into.guard(held, Repr::I32, (integer, &[]), (neither, &[]))
        .map_err(machine)?;
    into.switch_to(integer);
    let widened = into.to_f64(as_integer).map_err(machine)?;
    into.jump(join, &[widened]).map_err(machine)?;

    into.switch_to(neither);
    into.trap(rts_cranelift::ir::TrapCode::Unreachable);
    into.switch_to(join);
    Ok(result)
}

fn machine(held: impl std::fmt::Debug) -> String {
    format!("{held:?}")
}

impl<'a> JsMachine<'a> {
    /// A boundary over one function's inferred types, with no fall available.
    pub fn new(domain: &'a Js, types: Types<Type>) -> Self {
        Self {
            domain,
            types,
            generic: None,
            shared: None,
            receiver: None,
            incoming: Vec::new(),
            names: None,
            reraise: std::collections::BTreeMap::new(),
            in_cleanup: false,
            tail: std::collections::BTreeSet::new(),
            sloppy: false,
            method_reads: std::collections::BTreeSet::new(),
            body_entry: None,
            thrown_address: None,
            unbound: std::collections::BTreeSet::new(),
            parks: false,
            from_constant: std::collections::BTreeMap::new(),
        }
    }

    /// The same, paired with the generic body a guard falls to.
    ///
    /// # Why the pairing is given rather than discovered
    ///
    /// Because the two tiers of one function are two machine functions, and which id
    /// the generic one got is whoever declared it. `rts-host` is the crate that may
    /// name all three layers at once and is therefore where the pair is agreed --
    /// exactly as the entry-point symbols and the singleton numbering are.
    pub fn falling_to(domain: &'a Js, types: Types<Type>, generic: FuncId) -> Self {
        Self {
            domain,
            types,
            generic: Some(generic),
            shared: None,
            receiver: None,
            incoming: Vec::new(),
            names: None,
            reraise: std::collections::BTreeMap::new(),
            in_cleanup: false,
            tail: std::collections::BTreeSet::new(),
            sloppy: false,
            method_reads: std::collections::BTreeSet::new(),
            body_entry: None,
            thrown_address: None,
            unbound: std::collections::BTreeSet::new(),
            parks: false,
            from_constant: std::collections::BTreeMap::new(),
        }
    }

    /// The program's interner, for the pairing of a name to a machine key.
    pub fn naming_with(mut self, names: &'a mut crate::names::Names) -> Self {
        self.names = Some(names);
        self
    }

    /// Where a runtime operation may be declared, so a call to one can be emitted.
    ///
    /// Taken after construction rather than in it, because the two things a boundary may
    /// be given -- a generic twin to fall to and somewhere to declare a call -- are
    /// independent, and a constructor per combination is four constructors for two
    /// questions.
    /// Every parameter this activation arrived with, in the signature's order.
    ///
    /// One method for both facts the boundary needs from them -- which value is the
    /// receiver, and what a fall hands over -- so the convention is stated once. Parameter
    /// 0 is the environment and 1 is the receiver, which is what `emit/function.rs` fixes
    /// and `rts_core::entry::functions::invoke` calls on.
    pub fn with_incoming(mut self, params: &[MachineValue]) -> Self {
        self.receiver = params.get(1).copied();
        self.incoming = params.to_vec();
        self
    }

    /// That this body is NON-STRICT, which is the writer's mode every program write
    /// carries: a write the object refuses is a `TypeError` in strict code and nothing
    /// at all in sloppy -- `emit/property.rs::write_mode`, the same bit.
    pub fn sloppy(mut self, sloppy: bool) -> Self {
        self.sloppy = sloppy;
        self
    }

    /// The writer's mode as the runtime takes it: `1` for sloppy.
    pub(super) fn write_mode(&mut self, into: &mut FuncBuilder) -> MachineValue {
        self.word(into, u64::from(self.sloppy))
    }

    /// The reads a call takes its callee from, which read through the cache that
    /// reaches the prototype.
    pub fn method_reads_of(mut self, func: &rts_mir::cfg::Func) -> Self {
        self.method_reads = method_reads(func);
        self
    }

    /// That the throw check loads a flag whose ADDRESS is asked for once, in `entry`,
    /// rather than calling to ask for the flag after every raising call --
    /// `emit/expr.rs::body_flag`'s arrangement and its reason: five calls per pass of
    /// a loop that makes five raising calls. Not for a body that parks, which the
    /// running emitter leaves on the call too: a value in the entry block would have to
    /// survive every suspension.
    pub fn asking_once_in(mut self, entry: Option<rts_cranelift::ir::BlockId>) -> Self {
        self.body_entry = entry;
        self
    }

    /// The calls a return hands straight back, which go through `RuntimeOp::TailCall`
    /// -- see [`tail_positions`].
    pub fn tail_calls_of(mut self, func: &rts_mir::cfg::Func) -> Self {
        self.tail = tail_positions(func);
        self
    }

    /// That this function parks at every suspension, which is what `emit/expr.rs` does
    /// for `yield` in a generator and for `await` in a plain async function: hand the
    /// value to `GeneratorYield`, then the machine's suspension. Left unset, a suspension
    /// is refused -- an async GENERATOR's `await` drains where its `yield` parks, and one
    /// `Op::Suspend` cannot say which of the two it was.
    pub fn parking(mut self, parks: bool) -> Self {
        self.parks = parks;
        self
    }

    /// Which global reads name something nothing placed -- see the field.
    pub fn unbound_reads(mut self, reads: std::collections::BTreeSet<ValueId>) -> Self {
        self.unbound = reads;
        self
    }

    /// Where a runtime operation may be declared, so a call to one can be emitted.
    ///
    /// Taken after construction rather than in it, because the things a boundary may be
    /// given are independent and a constructor per combination is one per subset.
    pub fn declaring_in(self, shared: &'a mut Shared) -> Self {
        self.declaring_into(shared.parts())
    }

    /// The same, over registries somebody else owns -- the running emitter's, when a
    /// function of the program it is compiling is lowered through this stage instead.
    /// ONE set of registries for the program either way: a second numbering of a key, a
    /// literal or a function would reach the wrong thing rather than failing.
    pub fn declaring_into(mut self, parts: Parts<'a>) -> Self {
        self.shared = Some(parts);
        self
    }

    /// What a proved type is held in.
    ///
    /// `None` is "this slice has no answer", not "there is none". The generic tagged
    /// representation is what a complete lowering answers here, and answering it now
    /// would make every refusal below unreachable while emitting nothing that runs.
    pub fn repr_of(of: &Type) -> Option<Repr> {
        match of {
            Type::Int32 => Some(Repr::I32),
            Type::Double => Some(Repr::F64),
            Type::Bool(_) => Some(Repr::Bool),
            _ => None,
        }
    }

    /// What this value was proved to be.
    pub fn proved(&self, value: ValueId) -> &Type {
        self.types.of(value)
    }

    /// Whether every operand was proved to be a number.
    ///
    /// Asked of the MIR operands and not of the machine values, which is the whole reason
    /// the instruction travels: a machine value's representation is what this lowering
    /// CHOSE, and the question is what a pass PROVED. Reading the machine side would be
    /// this file marking its own homework.
    fn all_numeric(&self, of: &[ValueId]) -> bool {
        !of.is_empty()
            && of
                .iter()
                .all(|held| matches!(self.types.of(*held), Type::Int32 | Type::Double))
    }

    /// The operand in the double domain, converting a proved integer.
    ///
    /// The conversion is the LANGUAGE's decision and not a repair: the lattice said this
    /// operation answers a `Double`, so its operands belong in that domain. The machine
    /// refuses a mixed pair by design — `arith` takes two operands of one representation
    /// — and meeting that by converting is the front end doing its job rather than
    /// working around the check.
    fn as_double(
        &self,
        into: &mut FuncBuilder,
        held: MachineValue,
    ) -> Result<MachineValue, String> {
        match into.repr_of(held) {
            Repr::F64 => Ok(held),
            Repr::I32 => into.to_f64(held).map_err(machine),
            Repr::Tagged => unbox_number(into, held),
            other => Err(format!(
                "an operand proved numeric is held as {other:?}, which this slice cannot bring to the double domain"
            )),
        }
    }
}

impl JsMachine<'_> {
    /// A property read, through the site's memory of what it last saw.
    ///
    /// # The shape, and each block's reason for existing
    ///
    /// ```text
    ///   guard the receiver is a reference --- not one --------+
    ///            |                                            |
    ///   cached_get --- the layout changed ------------------- +
    ///            |                                            |
    ///          hit(value)                        slow: call the runtime
    ///            |                                            |
    ///            +------------------> join(value) <-----------+
    /// ```
    ///
    /// The GUARD is not optional and rule 11 of the machine's README is why: `cached_get`
    /// takes a proven reference, a graph value is generic, and narrowing out of the generic
    /// form can fail at run time -- so it is reachable only through a guard whose failure
    /// path cannot be omitted.
    ///
    /// THERE IS NO `hit` BLOCK, and that is taken from `emit/property.rs` rather than
    /// rediscovered: the machine PREPENDS the found value to whatever the hit call carries,
    /// so a forwarding block would be passing a value to itself. Three sites in that file
    /// did it and they were 403 of the 1 072 blocks in `bench/analytic.ts` holding nothing
    /// but a jump.
    ///
    /// WHAT STAYS ON THE SLOW PATH is everything that is not a named property of this
    /// receiver's own layout: an array element, a typed array's byte, a string's character,
    /// an inherited property, an accessor, a proxy. Each is answered correctly there, which
    /// is what makes the fast path allowed to be narrow.
    fn cached_read(
        &mut self,
        into: &mut FuncBuilder,
        object: MachineValue,
        key_operand: MachineValue,
        inherited: bool,
    ) -> Result<MachineValue, String> {
        // THE KEY AS A NUMBER FIXED WHILE COMPILING, recovered from the operand. A read
        // whose key the program COMPUTED is a different operation -- `cached_get_keyed` --
        // and refusing here rather than guessing is what keeps the two apart.
        let key = self.key_from_operand(key_operand)?;
        self.read_through_cache(into, object, key, key_operand, inherited)
    }

    /// A key the compiler fixed, minted from the program's one registry.
    fn key_of(&mut self, name: crate::names::Name) -> Result<rts_cranelift::shape::Key, String> {
        let Some(names) = self.names.as_deref_mut() else {
            return Err("a cached access needs the program's interner for its key".to_owned());
        };
        let Some(shared) = self.shared.as_mut() else {
            return Err("a cached access needs the program's key registry".to_owned());
        };
        Ok(names.key(name, &mut shared.keys))
    }

    /// The key a spelling the LANGUAGE chose has, and the operand the runtime is
    /// handed for it on a slow path.
    ///
    /// Interned here rather than declared in the graph, because the program never
    /// wrote it -- the same reason `JsConst::WellKnown` gives for the keys a
    /// `for`-`of` reads.
    fn fixed_key(
        &mut self,
        into: &mut FuncBuilder,
        spelled: &str,
    ) -> Result<(rts_cranelift::shape::Key, MachineValue), String> {
        let Some(names) = self.names.as_deref_mut() else {
            return Err(format!("the key {spelled} needs the program's interner"));
        };
        let name = names.intern(spelled);
        let key = self.key_of(name)?;
        let held = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(key.index() as u64),
        });
        Ok((key, into.use_const(held)))
    }

    /// The read itself, once the key is known -- through the cache that also reaches
    /// what the receiver inherits where `inherited` says the site is a method's.
    fn read_through_cache(
        &mut self,
        into: &mut FuncBuilder,
        object: MachineValue,
        key: rts_cranelift::shape::Key,
        key_operand: MachineValue,
        inherited: bool,
    ) -> Result<MachineValue, String> {

        let receiver = match into.repr_of(object) {
            Repr::Tagged => object,
            _ => into.widen(object),
        };
        let reference = into.create_block();
        let narrowed =
            into.add_block_param(reference, Repr::Ref(rts_cranelift::repr::RefKind::Opaque));
        let slow = into.create_block();
        let join = into.create_block();
        let result = into.add_block_param(join, Repr::Tagged);

        into.guard(
            receiver,
            Repr::Ref(rts_cranelift::repr::RefKind::Opaque),
            (reference, &[]),
            (slow, &[]),
        )
        .map_err(machine)?;

        into.switch_to(reference);
        let cache = into.declare_cache();
        match inherited {
            true => into.cached_get_indirect(narrowed, key, cache, (join, &[]), (slow, &[])),
            false => into.cached_get(narrowed, key, cache, (join, &[]), (slow, &[])),
        }
        .map_err(machine)?;

        into.switch_to(slow);
        let answered = self.call_runtime(
            into,
            crate::runtime::RuntimeOp::GetProperty,
            &[receiver, key_operand],
        )?;
        // AFTER the throw check, which `call_runtime` leaves the builder past -- so the jump
        // lands on the path where nothing was thrown rather than on the one that re-raised.
        into.jump(join, &[answered]).map_err(machine)?;

        into.switch_to(join);
        Ok(result)
    }

    /// Calls a runtime operation, coercing each argument and rechecking a throw.
    ///
    /// Shared by the entry-point arm and by the primitives that are really calls, because
    /// those differ only in which table named the operation -- and a second copy of the
    /// widening rule, the arity check and the throw check is three chances to get one of
    /// them wrong in one of the two.
    /// Installs an OWN property, through the site's memory of what it last saw.
    ///
    /// The mirror of [`Self::read_through_cache`], taken from `emit/property.rs`'s
    /// `emit_define`: the fast path is a cached store into a slot the layout already
    /// has, and a miss -- a key the object does not own yet, which is a shape
    /// transition no site can remember -- goes to `DefineField`.
    ///
    /// A DEFINE and not a `[[Set]]`, which is the reason this exists beside a field
    /// write: an environment holds bindings, and a binding spelled `__proto__` or one
    /// an `Object.prototype` setter shares a name with must land as an own data
    /// property. `[[Set]]` would run the setter.
    ///
    /// Answers the value handed in, untouched: the graph types the write by what was
    /// written, so a proved double has to come back as the double it was rather than
    /// as the widened word the store took.
    fn define_through_cache(
        &mut self,
        into: &mut FuncBuilder,
        object: MachineValue,
        key: rts_cranelift::shape::Key,
        key_operand: MachineValue,
        value: MachineValue,
    ) -> Result<(), String> {
        self.store_through_cache(into, object, key, key_operand, value, true)
    }

    /// The same store with either miss: `define` installs an own property through
    /// `DefineField`, and otherwise `[[Set]]` runs through `SetProperty` -- which may
    /// reach a setter and a prototype, which a program's `o.x = v` means.
    ///
    /// `SetProperty` takes the writer's MODE -- see [`JsMachine::write_mode`].
    fn store_through_cache(
        &mut self,
        into: &mut FuncBuilder,
        object: MachineValue,
        key: rts_cranelift::shape::Key,
        key_operand: MachineValue,
        value: MachineValue,
        define: bool,
    ) -> Result<(), String> {
        let receiver = match into.repr_of(object) {
            Repr::Tagged => object,
            _ => into.widen(object),
        };
        let stored = coerced(into, value, Repr::Tagged)?;
        let reference = into.create_block();
        let narrowed =
            into.add_block_param(reference, Repr::Ref(rts_cranelift::repr::RefKind::Opaque));
        let slow = into.create_block();
        let join = into.create_block();

        into.guard(
            receiver,
            Repr::Ref(rts_cranelift::repr::RefKind::Opaque),
            (reference, &[]),
            (slow, &[]),
        )
        .map_err(machine)?;

        into.switch_to(reference);
        let cache = into.declare_cache();
        into.cached_set(narrowed, key, cache, stored, (join, &[]), (slow, &[]))
            .map_err(machine)?;

        into.switch_to(slow);
        match define {
            true => self.call_runtime(
                into,
                crate::runtime::RuntimeOp::DefineField,
                &[receiver, key_operand, stored],
            )?,
            false => {
                let mode = self.write_mode(into);
                self.call_runtime(
                    into,
                    crate::runtime::RuntimeOp::SetProperty,
                    &[receiver, key_operand, stored, mode],
                )?
            }
        };
        into.jump(join, &[]).map_err(machine)?;

        into.switch_to(join);
        Ok(())
    }

    /// A fresh environment: an object with the link and every captured binding
    /// defined, the link first -- `JsPrim::EnvNew` says why each key is defined at
    /// creation rather than on first write.
    fn environment_new(
        &mut self,
        into: &mut FuncBuilder,
        enclosing: MachineValue,
        keys: &[MachineValue],
    ) -> Result<MachineValue, String> {
        // THE WIDTH `emit/binding.rs` gives one: a slot per name plus the link.
        let width = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(keys.len() as u64 + 1),
        });
        let width = into.use_const(width);
        let built = self.call_runtime(into, crate::runtime::RuntimeOp::ObjectNew, &[width])?;
        let (link, link_operand) = self.fixed_key(into, crate::emit::OUTER)?;
        self.define_through_cache(into, built, link, link_operand, enclosing)?;

        let undefined = self.undefined(into)?;
        for key_operand in keys {
            let key = self.key_from_operand(*key_operand)?;
            self.define_through_cache(into, built, key, *key_operand, undefined)?;
        }
        Ok(built)
    }

    /// An `I64` the compiler fixed: a count, a mode, an index.
    fn word(&mut self, into: &mut FuncBuilder, bits: u64) -> MachineValue {
        let held = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(bits),
        });
        into.use_const(held)
    }

    /// `undefined`, as bits from the program's tag registry.
    fn undefined(&mut self, into: &mut FuncBuilder) -> Result<MachineValue, String> {
        let Some(shared) = self.shared.as_ref() else {
            return Err("`undefined` is bits from the program's tag registry, which this boundary was not given".to_owned());
        };
        let undefined = shared.model.singleton(crate::values::Singleton::Undefined);
        let held = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: Repr::Tagged,
            bits: rts_cranelift::ir::ScalarBits(undefined.word()),
        });
        Ok(into.use_const(held))
    }

    /// The key a declared key constant is, recovered from the operand it was made as.
    ///
    /// `MachineOps::prim` knows WHICH value an operand is but not what defined it, so
    /// the constant is remembered where it was made -- see `from_constant`.
    fn key_from_operand(&mut self, key_operand: MachineValue) -> Result<rts_cranelift::shape::Key, String> {
        let Some(index) = self.from_constant.get(&key_operand).copied() else {
            return Err(
                "a cached access needs a key fixed while compiling, and this one is a value"
                    .to_owned(),
            );
        };
        self.key_of_constant(index)
    }

    /// The key a declared constant names: the program's own spelling, or one the
    /// language fixed. The one place a constant becomes a `shape::Key`, so a key made as
    /// an operand and the same key recovered for a cached access cannot disagree.
    fn key_of_constant(&mut self, index: u32) -> Result<rts_cranelift::shape::Key, String> {
        let name = match self.domain.declared(index) {
            Some(JsConst::Key(name)) => *name,
            // A KEY THE LANGUAGE FIXED, which the program never wrote: interned here by
            // the one spelling `WellKnown::spelled` gives it, so it lands on the key the
            // runtime stores the same property under.
            Some(JsConst::WellKnown(which)) => {
                let spelled = which.spelled();
                let Some(names) = self.names.as_deref_mut() else {
                    return Err(format!("the key {spelled} needs the program's interner"));
                };
                names.intern(spelled)
            }
            _ => {
                return Err(format!(
                    "a cached access's key is a property name, and constant {index} is not one"
                ));
            }
        };
        self.key_of(name)
    }

    fn call_runtime(
        &mut self,
        into: &mut FuncBuilder,
        which: crate::runtime::RuntimeOp,
        args: &[MachineValue],
    ) -> Result<MachineValue, String> {
        let declared = which.signature().params;
        if args.len() != declared.len() {
            return Err(format!(
                "{which:?} declares {} parameters and the graph supplied {}",
                declared.len(),
                args.len()
            ));
        }
        let mut of_args = Vec::with_capacity(args.len());
        for (held, want) in args.iter().zip(&declared) {
            of_args.push(match into.repr_of(*held) == *want {
                true => *held,
                false => coerced(into, *held, *want)?,
            });
        }
        let Some(shared) = self.shared.as_mut() else {
            return Err(format!(
                "calling {which:?} needs somewhere to declare it, which is the host's agreement"
            ));
        };
        let callee = shared.calls.declare(&mut shared.funcs, which);
        let produced = into
            .call(&shared.funcs, callee, &of_args)
            .map_err(machine)?;
        let answered = produced
            .first()
            .copied()
            .ok_or_else(|| format!("{which:?} answered nothing, and the graph reads its result"))?;
        if which.can_raise() {
            self.recheck_throw(into)?;
        }
        Ok(answered)
    }

    /// Emits, after a call that can raise, the branch a throw in the callee takes.
    ///
    /// # Why every such call pays this
    ///
    /// A throw leaves ONE frame. The machine records it and returns rather than ending the
    /// program, so the frame above only learns what happened by asking -- which
    /// `emit/expr.rs` states and which this boundary did not do, and therefore refused
    /// every raising call rather than emitting one that ignored the answer.
    ///
    /// Re-raising puts the value back into the machine's own hands, which routes it to a
    /// handler in THIS function through the region tree, or finding none returns and lets
    /// the frame above ask in turn. That is the whole of cross-frame unwinding here.
    ///
    /// Leaves the builder in the block that CARRIES ON, so a caller's next instruction
    /// lands on the path where nothing was thrown.
    fn recheck_throw(&mut self, into: &mut FuncBuilder) -> Result<(), String> {
        // NOT INSIDE A CLEANUP, and this is `emit/expr.rs`'s `raise_if_thrown` decision
        // taken the same way for the same reason: the re-raise is a throw, a throw is an
        // exit a cleanup piece may not have, and the machine's verifier refuses it as
        // `CleanupDoesNotEnd` -- which it did, on the first `finally` holding a call. The
        // gap it leaves is the running engine's too, and `emit/protect.rs` names it: a
        // call inside a `finally` that throws does not propagate out of the cleanup.
        if self.in_cleanup {
            return Ok(());
        }
        let Some(shared) = self.shared.as_mut() else {
            return Err(
                "a raising call needs the throw check, which needs somewhere to declare two calls"
                    .to_owned(),
            );
        };
        let flag = match (self.thrown_address, self.body_entry) {
            (Some(address), _) => into.word_load(address).map_err(machine)?,
            (None, Some(entry)) => {
                let asked = shared
                    .calls
                    .declare(&mut shared.funcs, crate::runtime::RuntimeOp::ThrownAddress);
                let resume = into.current();
                into.switch_to(entry);
                let address = into.call(&shared.funcs, asked, &[]).map_err(machine);
                into.switch_to(resume);
                let address = *address?.first().ok_or("ThrownAddress answered nothing")?;
                self.thrown_address = Some(address);
                into.word_load(address).map_err(machine)?
            }
            (None, None) => {
                let asked = shared
                    .calls
                    .declare(&mut shared.funcs, crate::runtime::RuntimeOp::Thrown);
                let flag = into.call(&shared.funcs, asked, &[]).map_err(machine)?;
                *flag.first().ok_or("Thrown answered nothing")?
            }
        };
        let zero = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(0),
        });
        let zero = into.use_const(zero);
        let raised = into
            .compare(rts_cranelift::ir::CmpOp::Ne, flag, zero)
            .map_err(machine)?;

        // TERMINATE FIRST, THEN FILL, and the order is not incidental: `emit/expr.rs`
        // records an earlier draft that created the block, filled it, and only then
        // terminated the one it had left -- which compiled and reached Cranelift's
        // verifier with "uses value from non-dominating inst" on a real program.
        let region = into.current_region();
        let carrying_on = into.create_block();
        match self.reraise.get(&region).copied() {
            Some(built) => into
                .branch(raised, (built, &[]), (carrying_on, &[]))
                .map_err(machine)?,
            None => {
                let made = into.create_block();
                into.branch(raised, (made, &[]), (carrying_on, &[]))
                    .map_err(machine)?;
                into.switch_to(made);
                let taken = shared
                    .calls
                    .declare(&mut shared.funcs, crate::runtime::RuntimeOp::TakeThrown);
                let value = into.call(&shared.funcs, taken, &[]).map_err(machine)?;
                let value = *value.first().ok_or("TakeThrown answered nothing")?;
                into.throw(crate::emit::protect::JS_THROW, value);
                self.reraise.insert(region, made);
            }
        }
        into.switch_to(carrying_on);
        Ok(())
    }
}

/// The values a call takes as its callee that a property read defined: `o.m(...)`.
pub fn method_reads(func: &rts_mir::cfg::Func) -> std::collections::BTreeSet<ValueId> {
    func.insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Call {
                callee: rts_mir::cfg::Callee::Dynamic(callee),
                receiver: Some(_),
                ..
            } => Some(*callee),
            _ => None,
        })
        .collect()
}

/// The calls in TAIL position: the last instruction of a block that returns its
/// answer, outside every protected region, in a function that does not park.
///
/// `emit/tail.rs` is the rule and its reasons, and this is the same rule read off the
/// graph: `return f(x)` records the call through `RuntimeOp::TailCall` and the frame
/// is gone before the callee runs, which is what lets a recursion a million deep run
/// flat. An ordinary call there is never WRONG, only deeper -- and deep enough, it is a
/// stack overflow, which is how the difference was found: seven files that pass under
/// the running emitter aborted when their functions came through this stage.
///
/// Outside a region because a handler, a `finally` or an iterator close runs on the way
/// out, which is work after the return; a parked frame is resumed by something other
/// than the door a tail call settles through.
pub fn tail_positions(func: &rts_mir::cfg::Func) -> std::collections::BTreeSet<ValueId> {
    let mut found = std::collections::BTreeSet::new();
    if func.may_suspend {
        return found;
    }
    for block in func.block_ids() {
        if func.region_of(block).is_some() {
            continue;
        }
        let held = func.block(block);
        let Some(rts_mir::cfg::Terminator::Return(Some(answered))) = held.terminator else {
            continue;
        };
        let Some(last) = held.insts.last() else {
            continue;
        };
        let inst = func.inst(*last);
        if inst.result == answered
            && matches!(
                inst.op,
                Op::Call {
                    callee: rts_mir::cfg::Callee::Dynamic(_),
                    ..
                }
            )
        {
            found.insert(answered);
        }
    }
    found
}

mod generic;
mod guarded;
mod ops;
mod reach;

pub use reach::{Parts, Shared, reaches_machine};
