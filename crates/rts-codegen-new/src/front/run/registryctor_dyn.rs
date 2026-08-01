//! RUNTIME overload dispatch for a Registry-class constructor.
//!
//! [`super::registryclass::Lowerer::emit_registry_ctor`] picks the constructor
//! overload from the registered candidates by ARITY, breaking a same-arity tie by
//! each argument's PROVEN `JsKind`. When nothing is proven — `function f(x) { new
//! C(x) }`, an `any` field, a value out of a `JSON.parse` — the front genuinely
//! cannot know which overload applies. It used to bail there, which is honest but
//! costs every real script that constructs from a variable.
//!
//! The choice does not have to be static. A `PolyValue` carries its type IN the
//! word, so "is this a string / a number / an object" is one mask + compare of
//! pure IR (no call, no allocation). This module emits that: one arm per
//! candidate, guarded by the test its own parameter `AbiType` implies, merging on
//! a block param. The LAST candidate is the fallback arm (no test) — some arm must
//! run, and a value matching none of the tests has to go somewhere; the registered
//! order decides, exactly as it does for a proven kind.
//!
//! DOCTRINE. Nothing here names a class. The discriminating argument index, the
//! per-arm test and the arm order all come from the candidates' `AbiType`
//! signatures — Registry DATA. A class that registers a new overload gets the
//! dispatch for free; a class with a single constructor never reaches this file.
//!
//! COST. The arms are straight-line IR the egraph can fold whenever the argument's
//! representation turns out to be provable after inlining, so a monomorphic call
//! site keeps paying nothing.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;

use cranelift_module::Module;

use rts_engine::abi::AbiType;

use crate::value;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{Lowerer, Val};
use super::registry::ResolvedCall;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Emit a runtime-tag dispatch over same-arity constructor overloads.
    ///
    /// `vals` are the arguments, ALREADY lowered once (each arm reuses them, so a
    /// side-effecting argument is evaluated exactly once — the arms are dominated
    /// by the block that produced them). Returns the constructed instance word,
    /// merged from every arm.
    pub(super) fn emit_registry_ctor_dynamic(
        &mut self,
        module: &mut dyn Module,
        class: &str,
        candidates: &[ResolvedCall],
        vals: &[Val],
    ) -> FrontResult<Value> {
        // The discriminating argument: the first position where the candidates'
        // parameter types disagree. If they agree everywhere, the overloads are
        // indistinguishable by value and the first registered one is the answer.
        let Some(idx) = (0..vals.len())
            .find(|&i| candidates.iter().any(|c| c.arg_abis[i] != candidates[0].arg_abis[i]))
        else {
            let call = candidates[0].clone();
            return self.finish_registry_ctor(module, class, &call, vals.to_vec());
        };
        // ARM ORDER. The last arm is untested, so which candidate ends up there
        // decides what a value matching NO test becomes — and JS already answers
        // that: for a one-argument constructor the spec tries the STRING reading
        // only for an actual string and otherwise applies `ToNumber`. So arms are
        // ranked by how narrow their coercion is — `StrPtr` (`ToString`, which
        // would turn `null` into `"null"`) first, `Handle` next, a numeric
        // parameter (`ToNumber`, JS's own default) last. The sort is STABLE, so
        // candidates of equal rank keep their registered order.
        let mut arms: Vec<ResolvedCall> = candidates.to_vec();
        arms.sort_by_key(|c| coercion_rank(c.arg_abis[idx]));
        if arms.len() < 2 {
            return unsupported!(
                "`new {class}(x)` — the overload depends on the runtime type and \
                 fewer than two candidates survived"
            );
        }
        let disc = self.box_value(vals[idx]);
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        let last = arms.len() - 1;
        for (i, call) in arms.iter().enumerate() {
            if i == last {
                // Fallback arm: no test, so every value reaches some constructor.
                let w = self.finish_registry_ctor(module, class, call, vals.to_vec())?;
                self.builder.ins().jump(merge, &[w.into()]);
                break;
            }
            let Some(pred) = emit_abi_tag_test(&mut self.builder, call.arg_abis[idx], disc) else {
                // No test exists for this parameter type (a `PolyValue` param
                // accepts anything, `Void` accepts nothing) — it cannot guard an
                // arm. Take it unconditionally rather than emit a wrong guard.
                let w = self.finish_registry_ctor(module, class, call, vals.to_vec())?;
                self.builder.ins().jump(merge, &[w.into()]);
                break;
            };
            let then_b = self.builder.create_block();
            let else_b = self.builder.create_block();
            self.builder.ins().brif(pred, then_b, &[], else_b, &[]);
            self.builder.switch_to_block(then_b);
            self.builder.seal_block(then_b);
            let w = self.finish_registry_ctor(module, class, call, vals.to_vec())?;
            self.builder.ins().jump(merge, &[w.into()]);
            self.builder.switch_to_block(else_b);
            self.builder.seal_block(else_b);
        }
        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        Ok(self.builder.block_params(merge)[0])
    }
}

/// How NARROW the coercion a parameter `AbiType` applies is: lower ranks are
/// tested first, so the widest one ends up as the untested fallback arm. See the
/// ARM ORDER note in [`Lowerer::emit_registry_ctor_dynamic`].
fn coercion_rank(abi: AbiType) -> u8 {
    match abi {
        AbiType::StrPtr => 0,
        AbiType::Handle => 1,
        AbiType::Bool => 2,
        AbiType::F64 | AbiType::I64 | AbiType::U64 | AbiType::I32 => 3,
        AbiType::PolyValue | AbiType::Void => 4,
    }
}

/// The runtime test a parameter `AbiType` implies on a `PolyValue` word, as pure
/// IR. `None` when the type discriminates nothing.
fn emit_abi_tag_test(
    builder: &mut FunctionBuilder,
    abi: AbiType,
    word: Value,
) -> Option<Value> {
    match abi {
        AbiType::StrPtr => Some(emit_has_tag(builder, word, value::TAG_STR)),
        // A number is EITHER an inline double or a tagged int32 — both must pass,
        // or `new C(1)` (a small int) would miss the numeric overload.
        AbiType::F64 | AbiType::I64 | AbiType::U64 | AbiType::I32 => {
            let is_double = value::emit_is_double(builder, word);
            let is_int = emit_has_tag(builder, word, value::TAG_INT32);
            Some(builder.ins().bor(is_double, is_int))
        }
        // A `Handle` param takes a heap value: an object, an array-backed handle
        // or a string handle all reach it through the marshal.
        AbiType::Handle => {
            let is_obj = emit_has_tag(builder, word, value::TAG_OBJECT);
            let is_str = emit_has_tag(builder, word, value::TAG_STR);
            Some(builder.ins().bor(is_obj, is_str))
        }
        AbiType::Bool | AbiType::PolyValue | AbiType::Void => None,
    }
}

/// `is_boxed(word) && tag(word) == tag` as pure IR (an `i8` boolean).
fn emit_has_tag(builder: &mut FunctionBuilder, word: Value, tag: u64) -> Value {
    let boxed = value::emit_is_boxed(builder, word);
    let shifted = builder.ins().ushr_imm(word, value::TAG_SHIFT as i64);
    let t = builder.ins().band_imm(shifted, value::TAG_MASK as i64);
    let eq = builder.ins().icmp_imm(IntCC::Equal, t, tag as i64);
    builder.ins().band(boxed, eq)
}
