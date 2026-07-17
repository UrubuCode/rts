//! Uniform-ABI THUNK generation for first-class function values (P4.6).
//!
//! A function used as a VALUE is invoked indirectly through the FIXED uniform
//! signature (the owner's mandated calling convention):
//!
//! ```text
//! extern "C" fn(env: u64, a0: u64, a1: u64, a2: u64, a3: u64, rest: u64) -> u64
//! ```
//!
//! A leading `env` PolyValue word (P5.7 — a closure's captured-value array word,
//! or `undefined` for a non-capturing function), then 4 positional PolyValue words
//! + a `rest` PolyValue (an array word of overflow args, or `undefined`),
//! returning one PolyValue word. The REAL function body keeps its native typed
//! signature ([`FnSig`]); a per-function THUNK bridges the two: it unpacks `a0..a3`
//! (and, for a `>4`-param / `...rest` callee, the overflow from the `rest` array)
//! into the body's real param reprs, `call`s the body, and boxes the result back
//! to a PolyValue word.
//!
//! ## Closures (P5.7)
//!
//! A CLOSURE's synthesized function has its captured free vars PREPENDED as leading
//! params (Tagged). For such a thunk the leading `capture_count` real params are
//! filled from `env[i]` (`VEC_GET`) instead of from `a0..a3`; the remaining real
//! params (the arrow's own declared params) come from `a0..a3`/`rest` at position
//! `i - capture_count`. A non-capturing thunk has `capture_count = 0` and ignores
//! `env`.
//!
//! `func_addr` (the reify step) points at the THUNK, never at the raw body — so
//! every indirect call goes through this fixed shape. The common ≤4-arg case
//! reads only `a0..a3` and never touches the rest array.

use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, Value, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};

use crate::repr::Repr;
use crate::value::{self, emit_marshal};

use crate::front::error::{FrontResult, Unsupported, unsupported};

use super::sig::FnSig;

/// The number of fixed positional slots in the uniform ABI (`a0..a3`).
const POSITIONAL: usize = 4;

/// Total uniform-ABI param count: leading `env` + `a0..a3` + `rest`.
const UNIFORM_PARAMS: usize = 1 + POSITIONAL + 1;

/// The Cranelift `Signature` of the uniform indirect-call ABI: six `i64`
/// (PolyValue word) params (`env, a0..a3, rest`), one `i64` return.
pub fn uniform_signature(module: &dyn Module) -> Signature {
    let mut sig = Signature::new(module.isa().default_call_conv());
    for _ in 0..UNIFORM_PARAMS {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

/// Declare the thunk symbol for `base_name` (the real function name) and return
/// its [`FuncId`]. The thunk name is `<base>__rtsn_thunk`. Declared `Local` so it
/// lives in the same module and `func_addr` can take its relocatable address.
pub fn declare_thunk(module: &mut dyn Module, base_name: &str) -> FrontResult<FuncId> {
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
/// real function `real_id` with signature `sig`. `capture_count` is the number of
/// LEADING real params filled from the closure `env` (`0` for a non-capturing
/// function). Bails explicitly for the cases outside this increment (a `...rest`
/// param, a non-coercible param repr).
pub fn define_thunk(
    module: &mut dyn Module,
    thunk_id: FuncId,
    real_id: FuncId,
    sig: &FnSig,
    capture_count: usize,
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

        let res = build_thunk_body(module, &mut fb, real_id, sig, capture_count);
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

/// Emit the thunk body: read the `capture_count` leading real params from the
/// closure `env`, unpack `a0..a3` (+ overflow from the rest array) into the
/// remaining real params, `call` the body, box the result.
fn build_thunk_body(
    module: &mut dyn Module,
    fb: &mut FunctionBuilder,
    real_id: FuncId,
    sig: &FnSig,
    capture_count: usize,
) -> FrontResult<()> {
    let entry = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    fb.seal_block(entry);

    // The six uniform params: env, a0..a3, rest.
    let block_params: Vec<Value> = fb.block_params(entry).to_vec();
    let env_word = block_params[0];
    let positional = &block_params[1..1 + POSITIONAL];
    let rest_word = block_params[1 + POSITIONAL];

    // A closure's leading `capture_count` real params come from `env[i]`; the rest
    // are the arrow's own declared params, riding a0..a3 / rest at the SHIFTED
    // position `i - capture_count` (the env params do not consume positional slots).
    let mut call_args: Vec<Value> = Vec::with_capacity(sig.params.len());
    for (i, &repr) in sig.params.iter().enumerate() {
        let word = if i < capture_count {
            // Captured var: read the snapshot from the env array.
            let idx = fb.ins().iconst(types::I64, i as i64);
            emit_marshal::emit_vec_get(module, fb, env_word, idx)
        } else if sig.rest_param == Some(i) {
            // The `...rest` param: PACK the remaining positional slots (trailing
            // `undefined`s trimmed — the uniform ABI carries no argc, so an
            // EXPLICIT trailing `undefined` argument is indistinguishable from an
            // absent one; a documented divergence) plus the overflow array into
            // one fresh array word.
            let pos = i - capture_count;
            let start = fb.ins().iconst(types::I64, pos as i64);
            let pack = emit_marshal::emit_call(
                module,
                fb,
                "__rtsadp_pack_rest",
                &[
                    positional[0],
                    positional[1],
                    positional[2],
                    positional[3],
                    rest_word,
                    start,
                ],
            );
            pack.expect("__rtsadp_pack_rest returns a value")
        } else {
            let pos = i - capture_count;
            if pos < POSITIONAL {
                positional[pos]
            } else {
                // Overflow arg: `rest[pos - POSITIONAL]` via the array VEC_GET (the
                // rest word is a TAG_OBJECT array PolyValue).
                let idx = fb.ins().iconst(types::I64, (pos - POSITIONAL) as i64);
                emit_marshal::emit_vec_get(module, fb, rest_word, idx)
            }
        };
        let arg = unbox_word_to_repr(fb, word, repr)?;
        call_args.push(arg);
    }

    // ASYNC fn-VALUE: o thunk NÃO roda o corpo inline (bloquearia o caller e
    // devolveria o valor cru em vez da Promise). Replica o CALL-SITE spawn
    // (`asyncspawn.rs`): empacota os args crus num Vec + kinds/meta e chama
    // `__rtsadp_promise_spawn`, devolvendo o handle da Promise PENDENTE boxado
    // como OBJECT word — o `.then` dinâmico e o `await` normalizam esse shape.
    // Capturas de closure já vieram prepostas em `call_args` (viajam como args
    // crus, captura-por-valor). Shapes fora do contrato do spawn (this/rest/>8
    // params) mantêm o thunk inline legado — o reify continua recusando esses.
    if sig.is_async && !sig.has_this && sig.rest_param.is_none() && sig.params.len() <= 8 {
        let vec_h = emit_marshal::emit_call(module, fb, "__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[])
            .expect("VEC_NEW returns a handle");
        for (v, &repr) in call_args.iter().zip(&sig.params) {
            let slot = match repr {
                Repr::Int32 => fb.ins().sextend(types::I64, *v),
                Repr::Float64 => fb.ins().bitcast(
                    types::I64,
                    cranelift_codegen::ir::MemFlags::new(),
                    *v,
                ),
                _ => *v,
            };
            emit_marshal::emit_call(
                module,
                fb,
                "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                &[vec_h, slot],
            );
        }
        // kinds/meta: o MESMO empacotamento do asyncspawn (0=i64, 1=f64-bits,
        // 2=bool; meta = box_kind<<16 | nparams<<8 | ret_kind).
        let mut kinds_packed: u64 = 0;
        for (i, &repr) in sig.params.iter().enumerate() {
            let k: u64 = match repr {
                Repr::Float64 => 1,
                Repr::Bool => 2,
                _ => 0,
            };
            kinds_packed |= k << (8 * i);
        }
        let ret_kind: u64 = match sig.ret {
            Some(Repr::Float64) => 1,
            Some(Repr::Bool) => 2,
            _ => 0,
        };
        let box_kind: u64 = match sig.ret {
            Some(Repr::Float64) => 1,
            Some(Repr::Bool) => 2,
            Some(Repr::Int32) | Some(Repr::Int64) => 0,
            _ => 3,
        };
        let meta = (box_kind << 16) | ((sig.params.len() as u64) << 8) | ret_kind;
        let kinds_word = fb.ins().iconst(types::I64, kinds_packed as i64);
        let meta_word = fb.ins().iconst(types::I64, meta as i64);
        let real_ref = module.declare_func_in_func(real_id, fb.func);
        let fn_addr = fb.ins().func_addr(types::I64, real_ref);
        let handle = emit_marshal::emit_call(
            module,
            fb,
            "__rtsadp_promise_spawn",
            &[fn_addr, vec_h, kinds_word, meta_word],
        )
        .expect("__rtsadp_promise_spawn returns a handle");
        let word = emit_marshal::emit_box_object_handle(module, fb, handle);
        fb.ins().return_(&[word]);
        return Ok(());
    }

    // Call the real body (its signature was attached at declaration time; the
    // FuncId carries it, so no re-derivation is needed here).
    let real_ref = module.declare_func_in_func(real_id, fb.func);
    let call = fb.ins().call(real_ref, &call_args);

    // Box the result back to a PolyValue word per the real return repr. A LAZY
    // GENERATOR outer returns a raw GenState HANDLE (`Int64`) — boxing it as a
    // double corrupts it for every dynamic consumer (a Tagged spread / for-of /
    // `next()` saw a number and threw "not iterable"); box it as an OBJECT word
    // instead, the same shape `__rtsadp_to_iter_array`/`iter_open` detect via
    // `Entry::GenState`.
    let ret_word = match sig.ret {
        Some(_) if sig.ret_lazy_gen => {
            let v = fb.inst_results(call)[0];
            emit_marshal::emit_box_object_handle(module, fb, v)
        }
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

/// Declare a per-class NEW-THUNK symbol (`<class>__rtsn_newthunk`) and return its
/// [`FuncId`]. The new-thunk is a uniform-ABI value that, when invoked, ALLOCATES
/// a fresh instance and runs the class constructor — so a class used as a VALUE
/// (`const C = Box; new C(7)`) reifies to this thunk's address. Sidesteps the
/// `reify_function` `has_this` bail: the ctor's `this` is synthesized HERE (the
/// allocation), never filled from a positional arg.
pub fn declare_new_thunk(module: &mut dyn Module, class_name: &str) -> FrontResult<FuncId> {
    let sig = uniform_signature(module);
    let name = new_thunk_name(class_name);
    module
        .declare_function(&name, Linkage::Local, &sig)
        .map_err(|e| Unsupported::new(format!("declare new-thunk `{name}`: {e}")))
}

/// The synthesized new-thunk symbol name for a user class `class`.
pub fn new_thunk_name(class: &str) -> String {
    format!("{class}__rtsn_newthunk")
}

/// Define a class NEW-THUNK body: allocate the instance (a `VEC` whose slot 0 is
/// `global_shape`, one `undefined` per field — exactly what `lower_new` emits),
/// run the constructor `ctor_id` with `this` = the instance and the explicit args
/// unpacked from `a0..a3` (+ `rest`), and RETURN the instance word (`TAG_OBJECT`),
/// NOT the ctor's own return. `ctor_sig.params[0]` is the `this` receiver; the
/// remaining params are the declared ctor args. A `...rest`/non-coercible param is
/// rejected the same way [`build_thunk_body`] does.
pub fn define_new_thunk(
    module: &mut dyn Module,
    new_thunk_id: FuncId,
    ctor_id: FuncId,
    ctor_sig: &FnSig,
    global_shape: u32,
    n_fields: usize,
) -> FrontResult<()> {
    let mut ctx = module.make_context();
    ctx.func.signature = uniform_signature(module);
    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let res =
            build_new_thunk_body(module, &mut fb, ctor_id, ctor_sig, global_shape, n_fields);
        match res {
            Ok(()) => fb.finalize(),
            Err(e) => {
                module.clear_context(&mut ctx);
                return Err(e);
            }
        }
    }
    module
        .define_function(new_thunk_id, &mut ctx)
        .map_err(|e| Unsupported::new(format!("define new-thunk: {e}")))?;
    module.clear_context(&mut ctx);
    Ok(())
}

/// Emit the new-thunk body: allocate the instance, fill `this` + ctor args, call
/// the ctor, return the instance word.
fn build_new_thunk_body(
    module: &mut dyn Module,
    fb: &mut FunctionBuilder,
    ctor_id: FuncId,
    ctor_sig: &FnSig,
    global_shape: u32,
    n_fields: usize,
) -> FrontResult<()> {
    let entry = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    fb.seal_block(entry);
    let block_params: Vec<Value> = fb.block_params(entry).to_vec();
    // uniform params: env, a0..a3, rest. `env` is unused (a class-value never
    // captures); the positional/rest carry the ctor's explicit args.
    let positional = &block_params[1..1 + POSITIONAL];
    let rest_word = block_params[1 + POSITIONAL];

    // ---- allocate the instance: VEC + slot0 shape-id + one `undefined` per field
    let obj_word = emit_marshal::emit_new_vec_object(module, fb);
    let id_word = fb.ins().iconst(
        types::I64,
        value::PolyValue::from_i32(global_shape as i32).raw() as i64,
    );
    emit_marshal::emit_vec_push(module, fb, obj_word, id_word);
    let undef = fb
        .ins()
        .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
    for _ in 0..n_fields {
        emit_marshal::emit_vec_push(module, fb, obj_word, undef);
    }

    // ---- ctor call args: params[0] = `this` (the instance), params[1..] from
    //      a0.. (the explicit ctor args), unboxed to each param's repr.
    let mut call_args: Vec<Value> = Vec::with_capacity(ctor_sig.params.len());
    for (i, &repr) in ctor_sig.params.iter().enumerate() {
        if i == 0 {
            // `this` is the freshly allocated instance (a Tagged object word).
            call_args.push(obj_word);
            continue;
        }
        let pos = i - 1; // arg position (param 1 → a0)
        let word = if pos < POSITIONAL {
            positional[pos]
        } else {
            let idx = fb.ins().iconst(types::I64, (pos - POSITIONAL) as i64);
            emit_marshal::emit_vec_get(module, fb, rest_word, idx)
        };
        call_args.push(unbox_word_to_repr(fb, word, repr)?);
    }
    let ctor_ref = module.declare_func_in_func(ctor_id, fb.func);
    fb.ins().call(ctor_ref, &call_args);

    // The instance word IS the result of `new` (the ctor's own return is ignored).
    fb.ins().return_(&[obj_word]);
    Ok(())
}

/// Unbox a uniform-ABI PolyValue `word` into a value of native `repr` for the
/// real call (the inverse of [`box_repr_to_word`]). A `Tagged` param keeps the
/// word verbatim. Pure IR (the egraph folds a box/unbox round-trip).
fn unbox_word_to_repr(fb: &mut FunctionBuilder, word: Value, repr: Repr) -> FrontResult<Value> {
    Ok(match repr {
        Repr::Tagged => word,
        // SOUND number decode (both the tagged-int32 AND the inline-double
        // encodings): a caller boxes an int arg as a DOUBLE whenever its repr was
        // Int64 (`box_value`), so the int32-only unbox read 0 from e.g.
        // `f.call(null, 21)`. Decode by tag, then truncate for the int reprs —
        // exactly the `coerce(Tagged → Int)` rule, pure IR the egraph folds.
        Repr::Float64 => value::emit_tagged_number_to_f64(fb, word),
        Repr::Int32 | Repr::Int64 => {
            let f = value::emit_tagged_number_to_f64(fb, word);
            fb.ins().fcvt_to_sint_sat(types::I64, f)
        }
        Repr::Bool => {
            // A Bool carrier is i64 0/1; recover it from the boolean singleton word
            // by comparing against the `true` singleton.
            let true_word = fb
                .ins()
                .iconst(types::I64, value::PolyValue::bool(true).raw() as i64);
            let b = fb.ins().icmp(
                cranelift_codegen::ir::condcodes::IntCC::Equal,
                word,
                true_word,
            );
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
