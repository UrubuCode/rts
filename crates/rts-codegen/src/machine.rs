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

use crate::domain::{Js, JsAssertion, JsPrim, Type};

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
    generic: Option<(FuncId, &'a FuncRegistry)>,
}

/// The machine's own failures travel out as the machine's words.
///
/// A lowering that paraphrased the layer below it would be a second account of one fact,
/// and the two would disagree the first time either changed.
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
    pub fn falling_to(
        domain: &'a Js,
        types: Types<Type>,
        generic: FuncId,
        funcs: &'a FuncRegistry,
    ) -> Self {
        Self {
            domain,
            types,
            generic: Some((generic, funcs)),
        }
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

    fn declared(&mut self, _into: &mut FuncBuilder, index: u32) -> Result<MachineValue, String> {
        let named = match self.domain.declared(index) {
            Some(held) => format!("{held:?}"),
            None => format!("index {index}, which no table row answers"),
        };
        // A DECLARED CONSTANT IS A HEAP VALUE, mostly: a key, a text, a singleton, a
        // closure over a function of the module. Each needs the runtime's own numbering
        // for that kind of thing, and none is a number this slice can produce.
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
        if args.len() != 2 {
            return Err(format!(
                "{which:?} over {} operands has no form in this slice",
                args.len()
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

    fn fall(
        &mut self,
        into: &mut FuncBuilder,
        point: PointId,
        live: &[MachineValue],
    ) -> Result<(), String> {
        let Some((generic, funcs)) = self.generic else {
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
        let answered = into.call(funcs, generic, live).map_err(machine)?;
        into.ret(&answered);
        Ok(())
    }

    fn entry(
        &mut self,
        _into: &mut FuncBuilder,
        entry: EntryId,
        _args: &[MachineValue],
        _inst: &Inst,
    ) -> Result<MachineValue, String> {
        let named = match self.domain.entry_meaning(entry) {
            Some(held) => format!("{held:?}"),
            None => format!("{entry:?}, which no table row answers"),
        };
        // An entry point is reached by ADDRESS, and which address is `rts-host`'s
        // agreement rather than this crate's -- the entry table is wired and asserted
        // there for exactly that reason.
        Err(format!(
            "calling {named} needs the host's agreement about where it lives"
        ))
    }
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
) -> Result<(), rts_mir::lower::Unlowerable> {
    use rts_cranelift::ir::{FuncRegistry, Function, Signature};
    use rts_cranelift::types::TypeRegistry;

    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
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
    let signature = Signature {
        params,
        returns,
        ..Signature::default()
    };
    // THE GENERIC TWIN, when the caller has one. Declared with the same signature
    // because a fall hands over the same arguments and answers what the other tier
    // answers -- the two bodies of one function agree about their shape by definition.
    let twin = generic.map(|_| {
        let sig = funcs.declare_signature(signature.clone());
        funcs.declare_function(sig)
    });
    let mut machine = Function::new(signature);
    let entry = machine.entry;
    let start: Vec<_> = machine
        .block(entry)
        .expect("a function has an entry block")
        .params
        .clone();
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut machine, &types, entry);
    let mut ops = match twin {
        Some(twin) => JsMachine::falling_to(domain, inferred, twin, &funcs),
        None => JsMachine::new(domain, inferred),
    };
    rts_mir::lower::lower(func, &mut into, &mut ops, &start)
}
