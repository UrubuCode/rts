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
    /// One per FUNCTION here and not one per region, and that is correct only because this
    /// boundary refuses a region: where a `Throw` lands is decided by the region its block
    /// is in, so sharing between a site inside a `try` and one outside would route the
    /// outer one into a handler that never protected it. When regions arrive this becomes a
    /// map keyed by region, which is what `BodyState::reraise_in` already is.
    reraise: Option<rts_cranelift::ir::BlockId>,
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
            reraise: None,
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
            reraise: None,
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
    ) -> Result<MachineValue, String> {
        // THE KEY AS A NUMBER FIXED WHILE COMPILING, recovered from the operand. A read
        // whose key the program COMPUTED is a different operation -- `cached_get_keyed` --
        // and refusing here rather than guessing is what keeps the two apart.
        let Some(index) = self.from_constant.get(&key_operand).copied() else {
            return Err(
                "a cached read needs a key fixed while compiling, and this one is a value"
                    .to_owned(),
            );
        };
        let Some(JsConst::Key(name)) = self.domain.declared(index) else {
            return Err(format!(
                "a cached read's key is a property name, and constant {index} is not one"
            ));
        };
        let name = *name;
        let Some(names) = self.names.as_deref_mut() else {
            return Err("a cached read needs the program's interner for its key".to_owned());
        };
        let Some(shared) = self.shared.as_deref_mut() else {
            return Err("a cached read needs the program's key registry".to_owned());
        };
        let key = names.key(name, &mut shared.keys);

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
        into.cached_get(narrowed, key, cache, (join, &[]), (slow, &[]))
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
        let Some(shared) = self.shared.as_deref_mut() else {
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
        let Some(shared) = self.shared.as_deref_mut() else {
            return Err(
                "a raising call needs the throw check, which needs somewhere to declare two calls"
                    .to_owned(),
            );
        };
        let asked = shared
            .calls
            .declare(&mut shared.funcs, crate::runtime::RuntimeOp::Thrown);
        let flag = into.call(&shared.funcs, asked, &[]).map_err(machine)?;
        let flag = *flag.first().ok_or("Thrown answered nothing")?;
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
        let carrying_on = into.create_block();
        match self.reraise {
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
                self.reraise = Some(made);
            }
        }
        into.switch_to(carrying_on);
        Ok(())
    }
}

mod ops;
mod reach;

pub use reach::{Shared, reaches_machine};
