//! The four questions the machine asks, answered.
//!
//! Apart from the rest of the boundary because `machine.rs` passed the 1000-line ceiling
//! the two engine crates take, and the seam is the one the file already had: `mod.rs` is
//! what the boundary IS -- the state it holds and the helpers that emit a call, a throw
//! check, a cached read -- and this is the `MachineOps` implementation that decides which
//! of them each question needs.

use rts_cranelift::ir::{CmpOp, FuncBuilder, NumOp, ValueId as MachineValue};
use rts_cranelift::repr::Repr;
use rts_mir::cfg::{EntryId, Inst, Op, Prim, ValueId};
use rts_mir::guard::{Assertion, PointId};
use rts_mir::lower::MachineOps;

use super::{JsMachine, Shared, coerced, machine};
use crate::domain::{JsAssertion, JsConst, JsPrim, Type};
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
            // NO THROW CHECK, and that is `CANNOT_RAISE` being load-bearing rather than
            // an omission: `StringConst` reads a table the host installed, which is why
            // it is on that list and why this call is the one a text constant can be.
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
            let held = into.use_const(held);
            // REMEMBERED, so a cached read can recover the `shape::Key` its key operand is.
            self.from_constant.insert(held, index);
            return Ok(held);
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

        // SOME ROWS ARE RUNTIME CALLS WEARING AN OPERATION'S CLOTHES, and this is the
        // first: `GlobalRead` reads a name no scope declares, through the global object,
        // and `RuntimeOp::GlobalGet` is what does it. It takes the key and answers the
        // value, so the graph's operand passes straight through.
        //
        // It became the largest single refusal the moment property keys started lowering --
        // 62 of `bench/` and 66 of `tests/` -- and was invisible before that, because
        // reading a global takes a key and every one of those functions stopped at the key.
        //
        // A PRIM and not an entry in the table, deliberately: the domain's own note says an
        // entry point would be the wrong shape for it, because what earns `Math.abs` one
        // instruction is a proof that nobody reassigned `Math`, which is a pass. So the
        // operation stays an operation in the graph and becomes a call here.
        if which == JsPrim::FieldRead
            && let [object, key] = args
        {
            return self.cached_read(into, *object, *key);
        }
        if which == JsPrim::GlobalRead {
            return self.call_runtime(into, crate::runtime::RuntimeOp::GlobalGet, args);
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
        let answered = produced
            .first()
            .copied()
            .ok_or_else(|| format!("{which:?} answered nothing, and the graph reads its result"))?;
        // THE CHECK, where the operation can raise. `can_raise` is the inverse of a list of
        // eight that `runtime::raising` holds, each naming the `rts-core` body it was read
        // against -- so this is not a guess about which operations throw.
        if which.can_raise() {
            self.recheck_throw(into)?;
        }
        Ok(answered)
    }
}
