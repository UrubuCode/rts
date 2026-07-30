//! DATA-DRIVEN dispatch of primordial `Array.prototype` methods by RUNTIME name.
//!
//! When `recv.m(args)` has an UNTRACKED array receiver — an `any`-typed local, a
//! `var` whose initializer arrived as an assignment, an element, a borrowed
//! `Array.prototype.m` value — the lowering cannot resolve `m` statically and
//! falls to `__rtsadp_dyn_method_call` / `__rtsadp_idx_call`. Those carried a
//! HAND-WRITTEN `match (name, argc)` covering six methods
//! (`push`/`pop`/`shift`/`join`/`indexOf`/`includes`), so everything else threw
//! `TypeError: <m> is not a function` on a perfectly ordinary array.
//!
//! This resolves the SAME [`MethodSpec`] rows the proven-receiver compile-time
//! path uses ([`super::super::dispatch::resolve_method`]), marshals the boxed
//! PolyValue arguments to the row's declared ABI, looks the row's symbol up in
//! the BAKED symbol table (`rts_runtime::symbol_table` — the single source of
//! truth the JIT vtable and the AOT symbol set both read), and calls it. One
//! generic path, no per-method row here: a method added to `ARRAY_ROWS` is
//! dispatchable dynamically the moment it exists.
//!
//! Every Array row's ABI is an INTEGER-register type (`U64` boxed word, `I64`
//! index/count, `Bool`, `Handle` string/vec handle), which is why one
//! `extern "C" fn(i64, …) -> i64` shape per arity covers the table. A row whose
//! ABI ever leaves that set (an `F64`/`StrPtr` argument) is REFUSED here rather
//! than mis-called — [`arg_is_int_reg`] is the gate.

use std::collections::HashMap;
use std::sync::OnceLock;

use rts_engine::AbiType;

use super::super::dispatch::{MethodSpec, RecvClass, resolve_method};
use super::{PolyValue, abi_adapter, genops};

/// Name → address of every symbol in the baked table. Built once; the table is
/// static and strictly ordered, so this is a pure re-index for O(1) lookup.
fn symbol_addrs() -> &'static HashMap<&'static str, usize> {
    static MAP: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    MAP.get_or_init(|| {
        rts_runtime::symbol_table::symbols()
            .into_iter()
            .map(|e| (e.name, e.ptr as usize))
            .collect()
    })
}

/// Whether `t` rides an integer register in the C ABI — the precondition for the
/// uniform transmute below. `F64`/`StrPtr`/`Void` do not.
fn arg_is_int_reg(t: AbiType) -> bool {
    matches!(
        t,
        AbiType::U64 | AbiType::I64 | AbiType::I32 | AbiType::Bool | AbiType::Handle
    )
}

/// Marshal one boxed PolyValue argument `w` to the row's declared `AbiType`.
/// `None` when the value cannot be represented in that ABI (never a guess).
fn marshal_arg(w: u64, want: AbiType) -> Option<i64> {
    match want {
        // A raw PolyValue WORD: element identity must survive verbatim (`indexOf`
        // compares the stored boxed word).
        AbiType::U64 => Some(w as i64),
        // An index / count: JS ToNumber then truncate, exactly what the proven
        // path's `numeric_to_i64` emits.
        AbiType::I64 | AbiType::I32 => {
            let f = genops::to_number(PolyValue::from_raw(w));
            Some(if f.is_nan() { 0 } else { f as i64 })
        }
        AbiType::Bool => Some(i64::from(genops::__rtsadp_to_boolean(w) != 0)),
        // A real STRING handle (a separator): ToString first, so a non-string
        // separator coerces like JS rather than refusing.
        AbiType::Handle => {
            let s = genops::__rtsadp_to_string(w);
            Some(abi_adapter::real_handle_of(PolyValue::from_raw(s)) as i64)
        }
        _ => None,
    }
}

/// Marshal the row's raw return back to a PolyValue word.
fn marshal_ret(raw: i64, spec: &MethodSpec) -> u64 {
    match spec.ret {
        // Already a PolyValue word (an element, or a whole array for the
        // array-returning rows).
        AbiType::U64 => raw as u64,
        AbiType::Bool => PolyValue::bool(raw != 0).raw(),
        // A real STRING handle (`join`, `toString`) → a TAG_STR word.
        AbiType::Handle => abi_adapter::poly_from_real_handle(raw as u64).raw(),
        // A count / index. `i32` covers every real array length; a value outside
        // it rides the double form rather than wrapping.
        AbiType::I64 | AbiType::I32 => {
            if let Ok(v) = i32::try_from(raw) {
                PolyValue::from_i32(v).raw()
            } else {
                PolyValue::from_f64(raw as f64).raw()
            }
        }
        _ => PolyValue::undefined().raw(),
    }
}

/// Dispatch `arr.<name>(args…)` through the Registry rows. `recv_handle` is the
/// receiver's real Vec handle; `args` are boxed PolyValue words.
///
/// `None` (fall through, never a wrong value) when: no row matches
/// `(name, argc)`; the row takes a CALLBACK (those need a reified function and
/// keep their own path); the row's ABI leaves the integer-register set; or the
/// symbol is absent from the baked table.
pub(super) fn dispatch(recv_handle: u64, name: &str, args: &[u64]) -> Option<u64> {
    let spec = resolve_method(RecvClass::Array, name, args.len())?;
    if spec.cb.is_some() {
        return None;
    }
    if !spec.args.iter().copied().all(arg_is_int_reg) || !arg_is_int_reg(spec.ret) {
        return None;
    }
    let addr = *symbol_addrs().get(spec.symbol)?;
    let mut slots: Vec<i64> = Vec::with_capacity(args.len() + 1);
    slots.push(recv_handle as i64);
    for (w, &want) in args.iter().zip(spec.args.iter()) {
        slots.push(marshal_arg(*w, want)?);
    }
    // SAFETY: `addr` is the address of a `#[rtse::abi]`/`extern "C"` function the
    // baked table resolved by the EXACT symbol the row names, and every parameter
    // plus the return was just checked to ride an integer register (see
    // `arg_is_int_reg`), so this transmute matches the callee's real signature.
    // The arity comes from the row itself, not from the call site.
    let raw = unsafe {
        match slots.len() {
            1 => {
                let f: extern "C" fn(i64) -> i64 = std::mem::transmute(addr);
                f(slots[0])
            }
            2 => {
                let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(addr);
                f(slots[0], slots[1])
            }
            3 => {
                let f: extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(addr);
                f(slots[0], slots[1], slots[2])
            }
            4 => {
                let f: extern "C" fn(i64, i64, i64, i64) -> i64 = std::mem::transmute(addr);
                f(slots[0], slots[1], slots[2], slots[3])
            }
            _ => return None,
        }
    };
    Some(marshal_ret(raw, &spec))
}
