//! Uniform-ABI THUNK generation for first-class function values (P4.6).
//!
//! A function used as a VALUE is invoked indirectly through the FIXED uniform
//! signature (the owner's mandated calling convention):
//!
//! ```text
//! extern "C" fn(a0: u64, a1: u64, a2: u64, a3: u64, rest: u64) -> u64
//! ```
//!
//! 4 positional PolyValue words + a `rest` PolyValue (an array word of overflow
//! args, or `undefined`), returning one PolyValue word. The REAL function body
//! keeps its native typed signature ([`FnSig`]); a per-function THUNK bridges the
//! two: it unpacks `a0..a3` (and, for a `>4`-param / `...rest` callee, the
//! overflow from the `rest` array) into the body's real param reprs, `call`s the
//! body, and boxes the result back to a PolyValue word.
//!
//! `func_addr` (the reify step) points at the THUNK, never at the raw body — so
//! every indirect call goes through this fixed shape. The common ≤4-arg case
//! reads only `a0..a3` and never touches the rest array.

use cranelift_codegen::ir::{types, AbiParam, InstBuilder, Signature, Value};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use crate::repr::Repr;
use crate::value::{self, emit_marshal};

use crate::front::error::{unsupported, FrontResult, Unsupported};

use super::sig::FnSig;

/// The number of fixed positional slots in the uniform ABI (`a0..a3`).
const POSITIONAL: usize = 4;

/// The Cranelift `Signature` of the uniform indirect-call ABI: five `i64`
/// (PolyValue word) params, one `i64` return.
pub fn uniform_signature(module: &dyn Module) -> Signature {
    let mut sig = Signature::new(module.isa().default_call_conv());
    for _ in 0..POSITIONAL + 1 {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

/// Declare the thunk symbol for `base_name` (the real function name) and return
/// its [`FuncId`]. The thunk name is `<base>__rtsn_thunk`. Declared `Local` so it
/// lives in the same module and `func_addr` can take its relocatable address.
pub fn declare_thunk(module: &mut JITModule, base_name: &str) -> FrontResult<FuncId> {
    let sig = uniform_signature(module);
    let name = thunk_name(base_name);
    module
        .declare_function(&name, Linkage::Local, &sig)
        .map_err(|e| Unsupported::new(format!("declare thunk `{name}`: {e}")))
}

/// The synthesized thunk symbol name for a real function `base`.
pub fn thunk_name(base: &str) -> String {
    format!("{base}__rtsn_thunk")
}

/// Define the body of the thunk `thunk_id` that bridges the uniform ABI to the
/// real function `real_id` with signature `sig`. Bails explicitly for the cases
/// outside this increment (a `...rest` param, a non-coercible param repr).
pub fn define_thunk(
    module: &mut JITModule,
    thunk_id: FuncId,
    real_id: FuncId,
    sig: &FnSig,
) -> FrontResult<()> {
    // A function with up to 4 declared params reads them from a0..a3; a function
    // with >4 params reads positional args 5.. from the `rest` ARRAY (see
    // `build_thunk_body`). A `...rest` (variadic) callee is NOT in this increment:
    // the FnSig has no variadic marker, arrow extraction already rejects variadic
    // arrow params, and a named-fn variadic surfaces as an arity mismatch at the
    // call site — so no variadic ever reaches a thunk to be mis-bound.
    let mut ctx = module.make_context();
    ctx.func.signature = uniform_signature(module);

    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);

        let res = build_thunk_body(module, &mut fb, real_id, sig);
        match res {
            Ok(()) => fb.finalize(),
            Err(e) => {
                module.clear_context(&mut ctx);
                return Err(e);
            }
        }
    }

    module
        .define_function(thunk_id, &mut ctx)
        .map_err(|e| Unsupported::new(format!("define thunk: {e}")))?;
    module.clear_context(&mut ctx);
    Ok(())
}

/// Emit the thunk body: unpack `a0..a3` (+ overflow from the rest array) into the
/// real params, `call` the body, box the result.
fn build_thunk_body(
    module: &mut JITModule,
    fb: &mut FunctionBuilder,
    real_id: FuncId,
    sig: &FnSig,
) -> FrontResult<()> {
    let entry = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    fb.seal_block(entry);

    // The five uniform params: a0..a3, rest.
    let block_params: Vec<Value> = fb.block_params(entry).to_vec();
    let positional = &block_params[..POSITIONAL];
    let rest_word = block_params[POSITIONAL];

    // For each real param, fetch its incoming PolyValue word (from a0..a3 or the
    // rest array) and coerce it to the param's native repr.
    let mut call_args: Vec<Value> = Vec::with_capacity(sig.params.len());
    for (i, &repr) in sig.params.iter().enumerate() {
        let word = if i < POSITIONAL {
            positional[i]
        } else {
            // Overflow arg: `rest[i - POSITIONAL]` via the array VEC_GET (the rest
            // word is a TAG_OBJECT array PolyValue).
            let idx = fb.ins().iconst(types::I64, (i - POSITIONAL) as i64);
            emit_marshal::emit_vec_get(module, fb, rest_word, idx)
        };
        let arg = unbox_word_to_repr(fb, word, repr)?;
        call_args.push(arg);
    }

    // Call the real body (its signature was attached at declaration time; the
    // FuncId carries it, so no re-derivation is needed here).
    let real_ref = module.declare_func_in_func(real_id, fb.func);
    let call = fb.ins().call(real_ref, &call_args);

    // Box the result back to a PolyValue word per the real return repr.
    let ret_word = match sig.ret {
        Some(ret) => {
            let v = fb.inst_results(call)[0];
            box_repr_to_word(fb, v, ret)
        }
        None => fb
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64),
    };
    fb.ins().return_(&[ret_word]);
    Ok(())
}

/// Unbox a uniform-ABI PolyValue `word` into a value of native `repr` for the
/// real call (the inverse of [`box_repr_to_word`]). A `Tagged` param keeps the
/// word verbatim. Pure IR (the egraph folds a box/unbox round-trip).
fn unbox_word_to_repr(fb: &mut FunctionBuilder, word: Value, repr: Repr) -> FrontResult<Value> {
    Ok(match repr {
        Repr::Tagged => word,
        Repr::Float64 => value::emit_unbox_double(fb, word),
        Repr::Int32 | Repr::Int64 => value::emit_unbox_int32(fb, word),
        Repr::Bool => {
            // A Bool carrier is i64 0/1; recover it from the boolean singleton word
            // by comparing against the `true` singleton.
            let true_word = fb
                .ins()
                .iconst(types::I64, value::PolyValue::bool(true).raw() as i64);
            let b = fb
                .ins()
                .icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, word, true_word);
            fb.ins().uextend(types::I64, b)
        }
        other => return unsupported!("thunk cannot unbox a param of repr {other:?}"),
    })
}

/// Box a native return value of `repr` into a PolyValue word (the uniform ABI
/// return). Mirrors [`super::lower::Lowerer::box_value`] but standalone (the thunk
/// has no `Lowerer`).
fn box_repr_to_word(fb: &mut FunctionBuilder, v: Value, repr: Repr) -> Value {
    match repr {
        Repr::Tagged => v,
        Repr::Int32 => {
            let i32v = fb.ins().ireduce(types::I32, v);
            value::emit_box_int32(fb, i32v)
        }
        Repr::Int64 => {
            let f = fb.ins().fcvt_from_sint(types::F64, v);
            value::emit_box_double(fb, f)
        }
        Repr::Float64 => value::emit_box_double(fb, v),
        Repr::Bool => {
            let f_word = fb
                .ins()
                .iconst(types::I64, value::PolyValue::bool(false).raw() as i64);
            let t_word = fb
                .ins()
                .iconst(types::I64, value::PolyValue::bool(true).raw() as i64);
            fb.ins().select(v, t_word, f_word)
        }
        // Ref kinds are not produced by the current function subset.
        other => {
            let _ = other;
            fb.ins()
                .iconst(types::I64, value::PolyValue::undefined().raw() as i64)
        }
    }
}
