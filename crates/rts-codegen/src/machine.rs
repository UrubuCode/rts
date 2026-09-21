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
    shared: Option<&'a mut Shared>,
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
        // ANYTHING INTO THE GENERIC FORM is always available: widening is what the
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

    /// Where a runtime operation may be declared, so a call to one can be emitted.
    ///
    /// Taken after construction rather than in it, because the things a boundary may be
    /// given are independent and a constructor per combination is one per subset.
    pub fn declaring_in(mut self, shared: &'a mut Shared) -> Self {
        self.shared = Some(shared);
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
            other => Err(format!(
                "an operand proved numeric is held as {other:?}, which this slice cannot bring to the double domain"
            )),
        }
    }
}

impl MachineOps for JsMachine<'_> {
    fn param_repr(&mut self, value: ValueId) -> Repr {
        // A PARAMETER CANNOT REFUSE, because the signature is built before any
        // instruction is lowered. So an unproven one takes the tagged representation and
        // the first primitive over it refuses by name -- which puts the message where the
        // reason is instead of on the signature.
        Self::repr_of(self.types.of(value)).unwrap_or(Repr::Tagged)
    }

    fn declared(&mut self, into: &mut FuncBuilder, index: u32) -> Result<MachineValue, String> {
        let named = match self.domain.declared(index) {
            Some(held) => format!("{held:?}"),
            None => format!("index {index}, which no table row answers"),
        };
        // A TEXT CONSTANT IS A CALL, and `emit/expr.rs` says why in one line: it is not a
        // constant here. `RuntimeOp::StringConst` takes the string's index and the runtime
        // holds the text, because a string is a heap value and a machine constant is bits.
        //
        // Through the PROGRAM's table and not a fresh one, which is the whole reason
        // `Shared` carries it: the index is an agreement with the runtime, which holds one
        // table, so a second numbering would reach the wrong string rather than failing.
        if let Some(JsConst::Text(text)) = self.domain.declared(index) {
            let units = text.units().to_vec();
            let Some(shared) = self.shared.as_deref_mut() else {
                return Err(format!(
                    "a text constant is a call to StringConst, which needs somewhere to declare it"
                ));
            };
            let which = shared.literals.intern(&units);
            let callee = shared
                .calls
                .declare(&mut shared.funcs, crate::runtime::RuntimeOp::StringConst);
            // THE INDEX AS AN `I64`, which is what the operation declares. A literal that
            // fits in an `i32` is declared `I32` by `rts-mir`'s own constant lowering --
            // deliberately, so the double domain can use it -- and the machine has no
            // integer widening, so this one is made at the width it is wanted at.
            let held = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
                repr: Repr::I64,
                bits: rts_cranelift::ir::ScalarBits(u64::from(which)),
            });
            let held = into.use_const(held);
            let produced = into.call(&shared.funcs, callee, &[held]).map_err(machine)?;
            return produced.first().copied().ok_or_else(|| {
                "StringConst answered nothing, and the graph reads its result".to_owned()
            });
        }
        // A PROPERTY KEY IS A NUMBER THE COMPILER RESOLVED, and the number is the whole of
        // it: `rts_cranelift::shape::Key` is opaque to the machine, which compares keys and
        // does nothing else with them. So this is a machine constant and not a call --
        // unlike a text, which is a heap value the runtime has to build.
        //
        // `I64`, because that is what every operation taking a key declares. `emit/` widens
        // a key only where the signature says `UNPROVEN`, and its own comment records the
        // call the machine refused when an earlier version widened it unconditionally.
        if let Some(JsConst::Key(name)) = self.domain.declared(index) {
            let name = *name;
            let Some(names) = self.names.as_deref_mut() else {
                return Err(
                    "a property key is minted from the program's interner, which this boundary was not given"
                        .to_owned(),
                );
            };
            let Some(shared) = self.shared.as_deref_mut() else {
                return Err(
                    "a property key is minted from the program's key registry, which this boundary was not given"
                        .to_owned(),
                );
            };
            let key = names.key(name, &mut shared.keys);
            let held = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
                repr: Repr::I64,
                bits: rts_cranelift::ir::ScalarBits(key.index() as u64),
            });
            return Ok(into.use_const(held));
        }
        // EVERY OTHER KIND IS STILL A HEAP VALUE the runtime numbers differently: a
        // a singleton, a closure over a function of the module. Each needs its own
        // agreement, and none is a number this slice can produce.
        Err(format!(
            "a declared constant needs the runtime's numbering for it: {named}"
        ))
    }

    fn prim(
        &mut self,
        into: &mut FuncBuilder,
        prim: Prim,
        args: &[MachineValue],
        inst: &Inst,
    ) -> Result<MachineValue, String> {
        let Some(which) = self.domain.meaning(prim) else {
            return Err(format!("{prim:?} is no row of this language's table"));
        };
        // THE OPERANDS FROM THE INSTRUCTION, and matching for a primitive is what makes
        // "this method is only handed one" an assertion instead of an assumption.
        let Op::Prim { args: of, .. } = &inst.op else {
            return Err(format!(
                "{which:?} was asked of an instruction that is not a primitive"
            ));
        };
        let of = of.clone();

        // THE ROWS WHOSE REQUIREMENT IS NOT "TWO NUMBERS", ahead of the gate below,
        // because the gate would refuse them for failing a condition they never had.
        //
        // `Truthy` over a proved boolean is the IDENTITY, and that is the whole of it:
        // this language's truth rule applied to something already a truth value asks
        // nothing and emits nothing. Getting here mattered more than it looks --
        // `truthy` is the second most common operation in `bench/` at 362, because every
        // `if (a < b)` writes one over a comparison's boolean, and refusing it stopped
        // every branch in the corpus at its condition.
        if which == JsPrim::Truthy
            && let [only] = of.as_slice()
            && matches!(self.types.of(*only), Type::Bool(_))
        {
            return Ok(args[0]);
        }

        // THE RECEIVER, which takes no operands and is not arithmetic, so it goes ahead of
        // both gates below. Reading it is reading a parameter: the signature declared one
        // and the caller said which.
        if which == JsPrim::ThisValue {
            return self.receiver.ok_or_else(|| {
                "the signature declared no receiver, and `this` is one of its parameters".to_owned()
            });
        }

        // ARITY FIRST, because it is the accurate reason for a row this slice has no
        // form for at any arity. Asked second, `NewArray` with no operands was reported
        // as "operands that were not proved numeric" followed by an empty list -- true
        // of a vacuous gate and useless to a reader.
        if args.len() != 2 {
            return Err(format!(
                "{which:?} over {} operands has no form in this slice",
                args.len()
            ));
        }
        if !self.all_numeric(&of) {
            // NAMED PER OPERAND, because "not proven" over two values is two different
            // situations and which one failed is the useful half.
            let unproven: Vec<String> = of
                .iter()
                .map(|held| format!("v{} is {:?}", held.0, self.types.of(*held)))
                .collect();
            return Err(format!(
                "{which:?} over operands that were not proved numeric: {}",
                unproven.join(", ")
            ));
        }

        let op = match which {
            // EVERY ARITHMETIC ROW ANSWERS A DOUBLE in this lattice, so every one of
            // these is a float instruction and there is no integer arm to choose. That is
            // measured rather than assumed: the first draft had one and it was dead.
            JsPrim::Subtract => Some(NumOp::Sub),
            JsPrim::Multiply => Some(NumOp::Mul),
            JsPrim::Divide => Some(NumOp::Div),
            // `%` IS NOT HERE, and the machine is what said so: `arith` answered
            _ => None,
        };
        let left = self.as_double(into, args[0])?;
        let right = self.as_double(into, args[1])?;
        if let Some(op) = op {
            return into.arith(op, left, right).map_err(machine);
        }
        match which {
            // `+` IS NOT ABOVE, and its absence is the point. Over two numbers it adds;
            // over anything else it may concatenate, and which one depends on a coercion
            // that can call user code. The table gives it a row of its own for that
            // reason, and a slice that lowered it beside the four would be lowering a
            // different operator on the strength of the operands looking alike.
            JsPrim::LessThan => into.compare(CmpOp::Lt, left, right).map_err(machine),
            // `a === b` over two proved numbers coerces nothing and can call nothing,
            // which is why it needs no proof beyond its operands'.
            JsPrim::StrictEquals => into.compare(CmpOp::Eq, left, right).map_err(machine),
            // THE BITWISE ROW would go the other way -- it answers a value that fits in
            // an `i32`, so its operands belong in the integer domain -- and it cannot be
            // emitted at all: the table deliberately holds five operators under one
            // `Prim`, so WHICH instruction is not in the graph. Refused rather than
            // guessed, and the table's own comment is where the fix belongs.
            JsPrim::Remainder => Err(
                "the remainder of two doubles is fmod, which the integer NumOp::Rem is not"
                    .to_owned(),
            ),
            JsPrim::BitwiseInt32 => Err(
                "the bitwise row is five operators under one Prim, so which instruction to emit is not in the graph"
                    .to_owned(),
            ),
            other => Err(format!(
                "{other:?} has no machine form in this slice, over proved numbers or otherwise"
            )),
        }
    }

    fn asserted_repr(&mut self, assertion: Assertion) -> Option<Repr> {
        match self.domain.asserted(assertion) {
            // A JavaScript number is a double, which is why the claim `number` asserts
            // this one and not the integer.
            Some(JsAssertion::IsDouble) => Some(Repr::F64),
            Some(JsAssertion::IsInt32) => Some(Repr::I32),
            // A STRING IS A REFERENCE to a heap value, and which layout it has is the
            // runtime's shape tree rather than a representation this can name. Answering
            // `Ref` of something invented would be a guard that narrowed to the wrong
            // thing and passed.
            Some(JsAssertion::IsStr) | Some(JsAssertion::HasShape(_)) | None => None,
        }
    }

    fn coerce(
        &mut self,
        into: &mut FuncBuilder,
        value: MachineValue,
        want: Repr,
    ) -> Result<MachineValue, String> {
        coerced(into, value, want)
    }
    fn fall(
        &mut self,
        into: &mut FuncBuilder,
        point: PointId,
        live: &[MachineValue],
    ) -> Result<(), String> {
        let Some(generic) = self.generic else {
            return Err(format!(
                "the fall at p{} has no generic body to land in, which is the pairing rts-host agrees",
                point.0
            ));
        };
        // THE WHOLE SIDE EXIT, for a guard at the entry: hand the same arguments to the
        // other tier and answer what it answers. Nothing is reconstructed because
        // nothing was built -- `rts_mir::lower` refuses a guard anywhere a local could
        // exist, so `live` is the parameters and the parameters are all there is.
        //
        // A TAIL CALL would be better and is not available here: `unwind`'s header says
        // a call in tail position discards its frame before control transfers, so it
        // cannot also be the call a handler is installed around. Until the specialised
        // body's protected regions are expressible this calls and returns, which costs
        // one frame on a path that is taken when a speculation failed -- the path whose
        // cost the whole arrangement is willing to pay.
        let Some(shared) = self.shared.as_deref_mut() else {
            return Err(format!(
                "the fall at p{} has no registry to name the generic body in",
                point.0
            ));
        };
        // WHAT THIS ACTIVATION ARRIVED WITH, and not `live`. The other tier has the same
        // signature, so entering it means handing over the same parameters -- the
        // convention's leading two included. `live` is the MIR's live set, which for an
        // entry guard is the program's parameters alone.
        let handed: Vec<MachineValue> = match self.incoming.is_empty() {
            true => live.to_vec(),
            false => self.incoming.clone(),
        };
        let answered = into
            .call(&shared.funcs, generic, &handed)
            .map_err(machine)?;
        into.ret(&answered);
        Ok(())
    }

    fn entry(
        &mut self,
        into: &mut FuncBuilder,
        entry: EntryId,
        args: &[MachineValue],
        _inst: &Inst,
    ) -> Result<MachineValue, String> {
        let Some(which) = self.domain.entry_meaning(entry) else {
            return Err(format!("{entry:?} is no row of this language's catalogue"));
        };
        // A CALL THAT CAN RAISE IS REFUSED, and this is the discipline that comes with
        // the pattern rather than the pattern itself. `emit/expr.rs` emits a
        // BRANCH-AND-RERAISE after every runtime call that can throw, and its own doc
        // says why every call site pays it: "a throw leaves ONE frame -- the machine
        // records it and returns rather than ending the program, so the frame above only
        // learns what happened by asking".
        //
        // This boundary does not ask. Emitting the call without the check would let a
        // program carry on with a garbage value after an exception that should have
        // propagated, which is the silent-wrong-answer class the honesty floor calls the
        // worst failure here. So it refuses, and the refusal names the operation.
        //
        // `can_raise` is the inverse of a list of eight that `runtime::raising` holds,
        // each naming the `rts-core` body it was read against -- so this is not a guess
        // about which operations throw.
        if which.can_raise() {
            return Err(format!(
                "{which:?} can raise, and this boundary emits no branch-and-reraise after a call"
            ));
        }
        let Some(Shared { funcs, calls, .. }) = self.shared.as_deref_mut() else {
            return Err(format!(
                "calling {which:?} needs somewhere to declare it, which is the host's agreement"
            ));
        };
        // THE SAME DECLARATION THE OLD EMITTER MAKES, through the same table: lazy, so a
        // compilation that never concatenates carries no relocation to the string path.
        let callee = calls.declare(funcs, which);
        // WIDENED PER PARAMETER and not unconditionally, which `emit/expr.rs` records as a
        // finding: a property key is a number the compiler resolved, declared `I64`, and
        // widening it produced a call the machine refused -- correctly, and with the
        // position named.
        let declared = which.signature().params;
        let mut of_args = Vec::with_capacity(args.len());
        for (held, want) in args.iter().zip(&declared) {
            of_args.push(match into.repr_of(*held) == *want {
                true => *held,
                false => coerced(into, *held, *want)?,
            });
        }
        if of_args.len() != declared.len() {
            return Err(format!(
                "{which:?} declares {} parameters and the graph supplied {}",
                declared.len(),
                args.len()
            ));
        }
        let produced = into.call(funcs, callee, &of_args).map_err(machine)?;
        // ONE RESULT, because that is what every row of this catalogue answers. A row that
        // answered none would leave the graph with a value nothing defines, which is a
        // different shape and not a missing case.
        produced
            .first()
            .copied()
            .ok_or_else(|| format!("{which:?} answered nothing, and the graph reads its result"))
    }
}

/// What every function of one module shares.
///
/// A `FuncId` is an agreement about a symbol, so it belongs to the PROGRAM and not to a
/// function -- which is how `rts-host::graph` builds a real one: `FuncRegistry` and
/// `RuntimeCalls` are created once and every file shares them. A registry per function
/// would hand each its own id for one symbol and nothing would notice.
#[derive(Default)]
pub struct Shared {
    /// Every function a call may name.
    pub funcs: rts_cranelift::ir::FuncRegistry,
    /// Which runtime operations have been declared, so each is declared once.
    pub calls: crate::runtime::RuntimeCalls,
    /// Every string the program holds, numbered as the runtime numbers it.
    ///
    /// Here and not per function for the same reason the registry is: a string's index
    /// is an agreement with the runtime, which holds ONE table. Two functions numbering
    /// their own would reach the wrong string rather than failing.
    pub literals: crate::runtime::Literals,
    /// What the machine calls each property, numbered once per program.
    ///
    /// `rts_cranelift::shape::KeyRegistry` hands out numbers and records no names, which
    /// is deliberate on its side: a key it understood would be a key it could be wrong
    /// about. The PAIRING of a name to a key lives on `Names`, and CLAUDE.md names this as
    /// the correct shape -- two tables of different lifetimes minting from ONE registry.
    pub keys: rts_cranelift::shape::KeyRegistry,
}

/// Does this graph reach the machine, and what stopped it if not?
///
/// # Why this is here and not only in a test
///
/// Because it is the only honest answer to "how far along is this stage". The share of
/// functions that lower to a GRAPH is 88% and says nothing about whether any of them
/// becomes code: a graph is refused at the machine boundary for reasons the graph itself
/// cannot show — an operand nothing proved, an entry point with no address, a region
/// with no tag.
///
/// So the two numbers are different questions and the second one is the one a reader
/// wants. Having it in the library rather than in a test is what lets `rts mir` report
/// it, and an instrument that can only be run by `cargo test` is an instrument nobody
/// runs.
///
/// # The signature comes from the LANGUAGE, one parameter at a time
///
/// Whatever the lattice proved about the graph's entry parameters is what the machine
/// function is declared to take, which is the agreement `param_repr` exists to state. A
/// caller that supplied its own would be measuring its own guess.
pub fn reaches_machine(
    func: &rts_mir::cfg::Func,
    domain: &Js,
    generic: Option<rts_mir::cfg::FuncId>,
    shared: &mut Shared,
    names: &mut crate::names::Names,
) -> Result<(), rts_mir::lower::Unlowerable> {
    use rts_cranelift::ir::{Function, Signature};
    use rts_cranelift::types::TypeRegistry;

    let types = TypeRegistry::new();
    let inferred = rts_mir::infer::infer(func, domain);
    let params: Vec<Repr> = func
        .block(func.entry())
        .params
        .iter()
        .map(|held| JsMachine::repr_of(inferred.of(*held)).unwrap_or(Repr::Tagged))
        .collect();
    // AND THE RETURN, from the same place. A function with no `Return` carrying a value
    // returns nothing, which is a signature with no results rather than one with an
    // invented result.
    let returns: Vec<Repr> = func
        .block_ids()
        .filter_map(|held| match func.block(held).terminator {
            Some(rts_mir::cfg::Terminator::Return(Some(value))) => Some(value),
            _ => None,
        })
        .next()
        .map(|held| JsMachine::repr_of(inferred.of(held)).unwrap_or(Repr::Tagged))
        .into_iter()
        .collect();
    // THE LANGUAGE'S CALLING CONVENTION, which `emit/function.rs` fixes and
    // `rts_core::entry::functions::invoke` calls on: parameter 0 is the environment and
    // parameter 1 is the receiver, both runtime values and therefore tagged. The
    // program's own parameters follow.
    //
    // Adopted here rather than left out, because a signature without them is one nothing
    // in this engine can call -- and the instrument would be measuring a function shape
    // that could never run.
    let leading = vec![Repr::Tagged, Repr::Tagged];
    let signature = Signature {
        params: leading.iter().copied().chain(params).collect(),
        returns,
        ..Signature::default()
    };
    // THE GENERIC TWIN, when the caller has one. Declared with the same signature
    // because a fall hands over the same arguments and answers what the other tier
    // answers -- the two bodies of one function agree about their shape by definition.
    let twin = generic.map(|_| {
        let sig = shared.funcs.declare_signature(signature.clone());
        shared.funcs.declare_function(sig)
    });
    let mut machine = Function::new(signature);
    let entry = machine.entry;
    let start: Vec<_> = machine
        .block(entry)
        .expect("a function has an entry block")
        .params
        .clone();
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut machine, &types, entry);
    // ONE REGISTRY FOR THE WHOLE MODULE, which is how `rts-host::graph` builds a real
    // program: `funcs` and `calls` are created once and every file and function share
    // them. A registry per function would give each one its own `FuncId` for one symbol,
    // and the instrument would never notice two functions failing to agree.
    //
    // Handed over whether or not there is a twin to fall to. Those were
    // mutually exclusive while the twin carried its own reference to the registry, which
    // is a limit nothing but that field imposed -- and it made the instrument measure a
    // falling function without entry points and a non-falling one with them.
    // THE GRAPH'S PARAMETERS ARE THE PROGRAM'S, so they map onto the TAIL of the
    // signature. Handing the whole list would bind the program's first parameter to the
    // environment, which is the kind of off-by-two that compiles.
    let mut ops = match twin {
        Some(twin) => JsMachine::falling_to(domain, inferred, twin),
        None => JsMachine::new(domain, inferred),
    }
    .declaring_in(shared)
    .naming_with(names)
    .with_incoming(&start);
    rts_mir::lower::lower(func, &mut into, &mut ops, &start[2..])
}
