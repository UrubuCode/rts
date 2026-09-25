//! The four questions the machine asks, answered.
//!
//! Apart from the rest of the boundary because `machine.rs` passed the 1000-line ceiling
//! the two engine crates take, and the seam is the one the file already had: `mod.rs` is
//! what the boundary IS -- the state it holds and the helpers that emit a call, a throw
//! check, a cached read -- and this is the `MachineOps` implementation that decides which
//! of them each question needs.

use rts_cranelift::ir::{FuncBuilder, ValueId as MachineValue};
use rts_cranelift::repr::Repr;
use rts_mir::cfg::{EntryId, Inst, Op, Prim, ValueId};
use rts_mir::guard::{Assertion, PointId};
use rts_mir::lower::MachineOps;

use super::{JsMachine, Parts, coerced, machine};
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
            let Some(shared) = self.shared.as_mut() else {
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
        // A SINGLETON IS BITS, and the bits come from the program's tag registry rather
        // than from anything this file knows: `SingletonId::word` is the value, and which
        // id each singleton has is `ValueModel::declare`'s answer over those tags.
        //
        // Two registries would encode one singleton two ways, which is why the model and
        // the tags are one field apart in `Shared` and never built separately.
        if let Some(JsConst::Singleton(which)) = self.domain.declared(index) {
            let which = *which;
            let Some(shared) = self.shared.as_mut() else {
                return Err(
                    "a singleton is bits from the program's tag registry, which this boundary was not given"
                        .to_owned(),
                );
            };
            let id = shared.model.singleton(which);
            let held = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
                repr: Repr::Tagged,
                bits: rts_cranelift::ir::ScalarBits(id.word()),
            });
            return Ok(into.use_const(held));
        }
        // A PROPERTY KEY IS A NUMBER THE COMPILER RESOLVED, and the number is the whole of
        // it: `rts_cranelift::shape::Key` is opaque to the machine, which compares keys and
        // does nothing else with them. So this is a machine constant and not a call --
        // unlike a text, which is a heap value the runtime has to build.
        //
        // `I64`, because that is what every operation taking a key declares. `emit/` widens
        // a key only where the signature says `UNPROVEN`, and its own comment records the
        // call the machine refused when an earlier version widened it unconditionally.
        if matches!(
            self.domain.declared(index),
            Some(JsConst::Key(_) | JsConst::WellKnown(_))
        ) {
            let key = self.key_of_constant(index)?;
            let held = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
                repr: Repr::I64,
                bits: rts_cranelift::ir::ScalarBits(key.index() as u64),
            });
            let held = into.use_const(held);
            // REMEMBERED, so a cached read can recover the `shape::Key` its key operand is.
            self.from_constant.insert(held, index);
            return Ok(held);
        }
        // A COUNT IS A MACHINE WORD, which is the whole of what the entry point taking it
        // declares; it is never a value of the language.
        if let Some(JsConst::Count(held)) = self.domain.declared(index) {
            let held = u64::from(*held);
            return Ok(self.word(into, held));
        }
        // A FUNCTION VALUE NEEDS THE MACHINE'S ID FOR THAT FUNCTION -- its code address
        // is what `ClosureNew` takes -- and this boundary compiles one function at a
        // time, so no other function of the module has an id here. That is the same
        // missing piece a direct call stops at (`NeedsCallee`): the module's machine
        // numbering, which only something compiling the whole module can hand out.
        //
        // Numbered now, when whoever compiles the module has declared its functions --
        // `Shared::number_module`. The address is `I64` and not a value: it is a machine
        // address nothing collects, which is `RuntimeOp::ClosureNew`'s own note.
        if let Some(JsConst::Function(which)) = self.domain.declared(index) {
            let which = *which;
            let numbered = self
                .shared
                .as_ref()
                .and_then(|shared| shared.module.get(which as usize).copied());
            let (Some(id), Some(shared)) = (numbered, self.shared.as_ref()) else {
                return Err(format!(
                    "a function value needs the machine id of f{which}, and this boundary compiles one function at a time -- the module numbering a direct call waits on too"
                ));
            };
            return into.func_addr(&shared.funcs, id).map_err(machine);
        }
        // EVERY OTHER KIND needs an agreement of its own, and none is a number this
        // slice can produce.
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

        // `ToNumber` OVER A PROVED NUMBER IS THE IDENTITY, for the reason `Truthy` over a
        // proved boolean is: the conversion asks nothing of what is already its answer.
        // The lattice agrees -- it answers the operand's own type for an Int32 and a
        // double otherwise -- so the representation passes through unchanged.
        if which == JsPrim::ToNumber
            && let [only] = of.as_slice()
            && matches!(self.types.of(*only), Type::Int32 | Type::Double)
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

        // THE ENVIRONMENT, which is five operations over an ordinary object and a
        // parameter -- `lower/environment.rs` has the layout and whose it is.
        //
        // The one this activation was made in is parameter 0 of the convention, read
        // the way the receiver is: the boundary was told the parameters.
        if which == JsPrim::EnclosingEnvironment {
            return self.incoming.first().copied().ok_or_else(|| {
                "the signature declared no environment, and it is parameter 0 of the convention"
                    .to_owned()
            });
        }
        if which == JsPrim::EnvNew
            && let [enclosing, keys @ ..] = args
        {
            return self.environment_new(into, *enclosing, keys);
        }
        if which == JsPrim::EnvOuter
            && let [environment] = args
        {
            let (link, operand) = self.fixed_key(into, crate::emit::OUTER)?;
            return self.read_through_cache(into, *environment, link, operand);
        }
        if which == JsPrim::EnvRead
            && let [environment, key] = args
        {
            return self.cached_read(into, *environment, *key);
        }
        if which == JsPrim::EnvWrite
            && let [environment, key_operand, value] = args
        {
            let key = self.key_from_operand(*key_operand)?;
            self.define_through_cache(into, *environment, key, *key_operand, *value)?;
            return Ok(*value);
        }
        if which == JsPrim::GlobalRead {
            let op = match self.unbound.contains(&inst.result) {
                true => crate::runtime::RuntimeOp::UnboundGlobalGet,
                false => crate::runtime::RuntimeOp::GlobalGet,
            };
            return self.call_runtime(into, op, args);
        }
        // A CLOSURE IS A CODE ADDRESS AND AN ENVIRONMENT, made by the runtime -- the same
        // call `emit/function.rs` makes, over the two operands the graph now carries.
        if which == JsPrim::MakeClosure {
            return self.call_runtime(into, crate::runtime::RuntimeOp::ClosureNew, args);
        }

        // PROVED NUMBERS: the instruction, where the row has one. `+` is among them, and
        // was absent on purpose: over anything but two numbers it may concatenate, and
        // choosing an instruction because the operands LOOK numeric would lower a
        // different operator. That concern is the gate this sits behind -- both operands
        // PROVED, by a literal or a guard -- and a proved number coerces nothing.
        if args.len() == 2 && self.all_numeric(&of) {
            let left = self.as_double(into, args[0])?;
            let right = self.as_double(into, args[1])?;
            if let Some(done) = self.numeric_instruction(into, which, left, right)? {
                return Ok(done);
            }
        }
        // UNPROVED NUMBERS: the same instruction behind a guard, falling to the call --
        // `guarded.rs` says why that is a lowering and not a speculation.
        if let Some(done) = self.guarded(into, which, &of, args)? {
            return Ok(done);
        }
        // A NEGATION OF A PROVED NUMBER flips the sign bit, which is not `0 - x`: that
        // answers `0` for `-0` where the language answers `-0`.
        if which == JsPrim::Negate
            && let [only] = of.as_slice()
            && matches!(self.types.of(*only), Type::Int32 | Type::Double)
        {
            let held = self.as_double(into, args[0])?;
            return into
                .float_unary(rts_cranelift::ir::FloatOp::Neg, held)
                .map_err(machine);
        }

        // EVERYTHING ELSE IS THE RUNTIME'S, the call the running engine makes for the same
        // operator -- `generic.rs` names each and says why a row is absent.
        if let Some(done) = self.generic(into, which, args)? {
            return Ok(done);
        }
        // WHAT IS LEFT has no form here at any operands. Named per operand, because the
        // types are the first thing a reader asks about.
        let unproven: Vec<String> = of
            .iter()
            .map(|held| format!("v{} is {:?}", held.0, self.types.of(*held)))
            .collect();
        Err(format!(
            "{which:?} over {} operands has no form in this slice: {}",
            args.len(),
            unproven.join(", ")
        ))
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

    fn in_cleanup(&mut self, inside: bool) {
        self.in_cleanup = inside;
    }

    fn exception_tag(&mut self) -> Option<rts_cranelift::unwind::Tag> {
        // ONE TAG, because one `catch` catches everything this language throws -- the
        // `NeedsHandlerTag` note said as much: the declaration is a line. It is the tag
        // the running engine throws and catches with, so a throw from this stage and a
        // handler from the other agree about what they are.
        Some(crate::emit::protect::JS_THROW)
    }

    fn hand_out(
        &mut self,
        into: &mut FuncBuilder,
        value: Option<MachineValue>,
    ) -> Option<Result<(), String>> {
        if !self.parks {
            return None;
        }
        let handed = (|| {
            // A BARE `yield` hands out `undefined`, which is where the language says so.
            let value = match value {
                Some(held) => held,
                None => self.undefined(into)?,
            };
            self.call_runtime(into, crate::runtime::RuntimeOp::GeneratorYield, &[value])
                .map(|_| ())
        })();
        Some(handed)
    }

    fn returned(
        &mut self,
        into: &mut FuncBuilder,
        value: MachineValue,
    ) -> Result<MachineValue, String> {
        // ONE TAGGED RESULT, which is what the convention declares: a caller that cannot
        // know the callee has to be able to receive whatever it answers. A proved double
        // or boolean is widened here, once, on the way out.
        coerced(into, value, Repr::Tagged)
    }

    fn coerce(
        &mut self,
        into: &mut FuncBuilder,
        value: MachineValue,
        want: Repr,
    ) -> Result<MachineValue, String> {
        // A TAGGED WORD WHERE THE LATTICE ASKS FOR A NUMBER: `rts_mir::lower` asks this
        // for a jump's argument into a block parameter the lattice typed `Double` or
        // `Int32`, and the value came through a guarded operator whose slow edge answers
        // tagged. The proof is sound, so the word is unboxed from either encoding --
        // `unbox_number`, which traps on anything else rather than reading garbage. An
        // `Int32` is exact from the double, being one by proof.
        match (into.repr_of(value), want) {
            (Repr::Tagged, Repr::F64) => super::unbox_number(into, value),
            (Repr::Tagged, Repr::I32) => {
                let double = super::unbox_number(into, value)?;
                into.to_int32(double).map_err(machine)
            }
            _ => coerced(into, value, want),
        }
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
        let Some(shared) = self.shared.as_mut() else {
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

    fn call_value(
        &mut self,
        into: &mut FuncBuilder,
        callee: MachineValue,
        receiver: Option<MachineValue>,
        args: &[MachineValue],
        inst: &Inst,
    ) -> Result<MachineValue, String> {
        // `RuntimeOp::Call` IS THE DOOR, and its shape is the convention written out:
        // callee, receiver, how many arguments were WRITTEN, which literal spells the
        // callee, then one slot per argument padded with `undefined`.
        //
        // The count is what lets a callee tell `f(undefined)` from `f()`. The name is what
        // used to be a crossing of its own -- `SetCallName`, measured at 2.3-2.9 ns on
        // every named call -- and `-1` spells a callee with nothing to name, which is what
        // this boundary always passes: the graph carries no callee spelling, deliberately,
        // because a name is for a diagnostic and a call is not.
        //
        // FOUR SLOTS, and `runtime/mod.rs` says why the arity is fixed: the machine has no
        // stack slot to put a real argument vector in, so `rts-core` keeps the vector in a
        // `Vec` of its own. Past them the arguments go in an ARRAY and `CallWithArgs`
        // takes it, which is the running emitter's `emit_call_with_name_as` -- and, as
        // there, never as a tail call: the vector is the activation's, not the caller's.
        if args.len() > crate::runtime::ARGUMENT_SLOTS {
            let vector = self.array_of(into, args)?;
            let receiver = match receiver {
                Some(held) => held,
                None => self.undefined(into)?,
            };
            return self.call_runtime(
                into,
                crate::runtime::RuntimeOp::CallWithArgs,
                &[callee, receiver, vector],
            );
        }
        let Some(shared) = self.shared.as_mut() else {
            return Err(
                "a call needs the program's tag registry for its undefined padding".to_owned(),
            );
        };
        let undefined = shared.model.singleton(crate::values::Singleton::Undefined);
        let padding = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: Repr::Tagged,
            bits: rts_cranelift::ir::ScalarBits(undefined.word()),
        });
        let padding = into.use_const(padding);
        let count = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(args.len() as u64),
        });
        let count = into.use_const(count);
        // `-1`, which spells a callee with nothing to name.
        let unnamed = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(u64::MAX),
        });
        let unnamed = into.use_const(unnamed);

        // NO RECEIVER IS `undefined` AND NOT ABSENT, because the door's arity is fixed and
        // a plain call is one whose receiver the callee decides. That is the language's own
        // rule: a non-strict function called with no receiver sees the global object, which
        // `emit/function.rs` substitutes once at the entry rather than at the call.
        let receiver = receiver.unwrap_or(padding);
        let mut of_args = vec![callee, receiver, count, unnamed];
        of_args.extend_from_slice(args);
        of_args.resize(4 + crate::runtime::ARGUMENT_SLOTS, padding);
        // A CALL A RETURN HANDS STRAIGHT BACK records itself rather than growing the
        // stack -- the same door with the same operands, `tail_positions` says which.
        let door = match self.tail.contains(&inst.result) {
            true => crate::runtime::RuntimeOp::TailCall,
            false => crate::runtime::RuntimeOp::Call,
        };
        self.call_runtime(into, door, &of_args)
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

        let Some(Parts { funcs, calls, .. }) = self.shared.as_mut() else {
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
