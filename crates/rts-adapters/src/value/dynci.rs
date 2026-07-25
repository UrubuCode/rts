//! Runtime dispatch of object-backed Registry class instance methods.
//!
//! When `recv.method(args)` has an UNTRACKED receiver (an array element, a
//! function parameter, any `any`-typed local whose Registry class the front-end
//! never proved), the lowering falls to `__rtsadp_dyn_method_call`, which reads
//! `method` off the receiver's prototype chain. Object-backed classes (`Hash`,
//! `Stats`, `StringDecoder`, `Channel` — an `Entry::Map` tagged `__rts_class`)
//! publish NO prototype methods; their methods are native `extern "C"` fns in the
//! Registry. So that path would throw `TypeError: method is not a function`.
//!
//! [`try_runtime_ci`] closes the gap: it reads the receiver's own `__rts_class`
//! tag, resolves the native method in the process-global
//! [`rts_engine::runtime_ci`] table (harvested from the same Registry the proven
//! compile-time path uses), marshals the boxed PolyValue args to the method's
//! declared ABI, and calls the real `fn_ptr`. The class comes from the VALUE, so
//! the engine never names a non-primordial class; it is pure data dispatch.

use rts_engine::runtime_ci::{lookup_ci, CiMethod};
use rts_engine::AbiType;
use rts_engine::heap::handles::{with_entry, Entry};
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::{abi_adapter, genops};
use super::PolyValue;

/// The runtime class name a receiver's methods dispatch under: an object-backed
/// `Entry::Map` `__rts_class` tag, a value-class `Entry::Rtse` class, OR a
/// PRIMITIVE that autoboxes to a value-class (a string → `"String"`; numbers /
/// bools follow once migrated). `None` when the receiver carries no such class.
fn class_tag_of(recv: u64) -> Option<String> {
    let v = PolyValue::from_raw(recv);
    // A PRIMITIVE string autoboxes to the `#[rtse::class("String", value)]`
    // value-class — its methods live in `runtime_ci` under `"String"`.
    if v.is_string() {
        return Some("String".to_string());
    }
    if !v.is_object() {
        return None;
    }
    let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    // A value-class WRAPPER object (`new String(x)` → `Entry::Rtse` classed
    // `"String"`; likewise a future `new Number()`/`new Boolean()`).
    if let Some(c) = rts_engine::heap::handles::rtse_class_of(h) {
        return Some(c.to_string());
    }
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("__rts_class").map(|&sh| abi_adapter::real_handle_to_string(sh as u64)),
        _ => None,
    })
}

/// Normalize a boxed PolyValue word to a raw runtime handle for a `Handle`-typed
/// arg (object/string/function → its real handle; anything else passes through
/// unchanged, matching the static marshal).
fn any_handle(word: u64) -> u64 {
    let v = PolyValue::from_raw(word);
    if v.is_object() || v.is_string() || v.is_function() {
        rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle())
    } else {
        word
    }
}

/// Coerce a boxed PolyValue word to an owned UTF-8 `String` for a `StrPtr` arg.
/// The owned buffer keeps the bytes alive for the duration of the native call
/// (the callee copies/reads them synchronously), independent of GC.
fn owned_str(word: u64) -> String {
    let s = genops::__rtsadp_to_string(word);
    abi_adapter::resolve_poly(PolyValue::from_raw(s))
}

/// ToNumber a PolyValue-word arg to `f64` (the engine's ONE ToNumber rule) — for
/// a value-class method's `f64` parameter (`slice`/`charAt`/`indexOf` position…).
fn genops_num(word: u64) -> f64 {
    genops::dyn_to_number(word)
}

/// Rebox a native return value per its ABI so the JIT sees a proper PolyValue.
fn rebox(ret: AbiType, raw: u64) -> u64 {
    match ret {
        AbiType::Void => PolyValue::undefined().raw(),
        AbiType::Bool => PolyValue::bool(raw != 0).raw(),
        AbiType::Handle | AbiType::U64 => genops::__rtsadp_box_handle_auto(raw),
        AbiType::I64 | AbiType::I32 => PolyValue::from_f64(raw as i64 as f64).raw(),
        AbiType::F64 => PolyValue::from_f64(f64::from_bits(raw)).raw(),
        // A `StrPtr`/`PolyValue` return is not produced by the current object-
        // backed methods; treat a raw word as already-boxed.
        _ => raw,
    }
}

/// Try to dispatch `recv.method(a0,a1,a2)` as a native object-backed instance
/// method. Returns `Some(result_word)` on success, `None` when `recv` carries no
/// `__rts_class` tag, the method is unknown at that arity, or its ABI shape is
/// not one this marshaller supports (the caller then falls back to its miss/throw
/// path). `js_argc` is the count of real (non-undefined) leading args.
pub fn try_runtime_ci(recv: u64, key: u64, a0: u64, a1: u64, a2: u64, js_argc: usize) -> Option<u64> {
    let class = class_tag_of(recv)?;
    let method = abi_adapter::resolve_poly(PolyValue::from_raw(key));
    // Full sig arity = receiver + explicit js args.
    let m: CiMethod = lookup_ci(&class, &method, js_argc + 1)?;
    let recv_h = any_handle(recv);

    use AbiType::{Bool, Handle, I64, PolyValue as APoly, StrPtr, Void};
    // SAFETY: `m.fn_ptr` is the exact native `extern "C"` fn the Registry
    // harvested for this (class, method, arity); the transmute target matches
    // `m.args`/`m.ret`, which we branch on. Any shape not enumerated returns
    // `None` (no call made).
    let out = unsafe {
        match (m.args.as_slice(), m.ret) {
            ([Handle], Handle) => {
                let f: extern "C" fn(u64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv_h))
            }
            ([Handle], Bool) => {
                let f: extern "C" fn(u64) -> i64 = std::mem::transmute(m.fn_ptr);
                rebox(Bool, f(recv_h) as u64)
            }
            ([Handle], Void) => {
                let f: extern "C" fn(u64) = std::mem::transmute(m.fn_ptr);
                f(recv_h);
                rebox(Void, 0)
            }
            ([Handle, Handle], Handle) => {
                let f: extern "C" fn(u64, u64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv_h, any_handle(a0)))
            }
            ([Handle, Handle], Void) => {
                let f: extern "C" fn(u64, u64) = std::mem::transmute(m.fn_ptr);
                f(recv_h, any_handle(a0));
                rebox(Void, 0)
            }
            ([Handle, APoly], Void) => {
                let f: extern "C" fn(u64, u64) = std::mem::transmute(m.fn_ptr);
                f(recv_h, a0);
                rebox(Void, 0)
            }
            ([Handle, APoly], Handle) => {
                let f: extern "C" fn(u64, u64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv_h, a0))
            }
            ([Handle, StrPtr], Handle) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv_h, s.as_ptr(), s.len() as i64))
            }
            ([Handle, StrPtr, StrPtr], Handle) => {
                let s0 = owned_str(a0);
                let s1 = owned_str(a1);
                let f: extern "C" fn(u64, *const u8, i64, *const u8, i64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv_h, s0.as_ptr(), s0.len() as i64, s1.as_ptr(), s1.len() as i64))
            }
            // `(event, listener)` — the EventEmitter surface every backend class
            // inherits (`on`/`once`/`off`/`prependListener`/…), which a listener
            // registered on an UNTRACKED receiver needs (`server.on('connection',
            // s => s.on('data', …))` — `s` is a param, so it has no static class).
            ([Handle, StrPtr, APoly], Handle) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64, u64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv_h, s.as_ptr(), s.len() as i64, a1))
            }
            ([Handle, StrPtr, APoly], Bool) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64, u64) -> i64 = std::mem::transmute(m.fn_ptr);
                rebox(Bool, f(recv_h, s.as_ptr(), s.len() as i64, a1) as u64)
            }
            ([Handle, StrPtr, APoly, APoly], Bool) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64, u64, u64) -> i64 = std::mem::transmute(m.fn_ptr);
                rebox(Bool, f(recv_h, s.as_ptr(), s.len() as i64, a1, a2) as u64)
            }
            ([Handle, StrPtr], Bool) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64) -> i64 = std::mem::transmute(m.fn_ptr);
                rebox(Bool, f(recv_h, s.as_ptr(), s.len() as i64) as u64)
            }
            ([Handle, StrPtr], Void) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64) = std::mem::transmute(m.fn_ptr);
                f(recv_h, s.as_ptr(), s.len() as i64);
                rebox(Void, 0)
            }
            ([Handle, StrPtr], I64) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64) -> i64 = std::mem::transmute(m.fn_ptr);
                rebox(I64, f(recv_h, s.as_ptr(), s.len() as i64) as u64)
            }
            // `write(chunk)` / `emit`-shaped returns.
            ([Handle, APoly], Bool) => {
                let f: extern "C" fn(u64, u64) -> i64 = std::mem::transmute(m.fn_ptr);
                rebox(Bool, f(recv_h, a0) as u64)
            }
            ([Handle, APoly, APoly], Handle) => {
                let f: extern "C" fn(u64, u64, u64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv_h, a0, a1))
            }
            ([Handle, APoly, APoly], Bool) => {
                let f: extern "C" fn(u64, u64, u64) -> i64 = std::mem::transmute(m.fn_ptr);
                rebox(Bool, f(recv_h, a0, a1) as u64)
            }
            ([Handle, APoly, APoly], Void) => {
                let f: extern "C" fn(u64, u64, u64) = std::mem::transmute(m.fn_ptr);
                f(recv_h, a0, a1);
                rebox(Void, 0)
            }
            ([Handle, APoly, APoly, APoly], Handle) => {
                let f: extern "C" fn(u64, u64, u64, u64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv_h, a0, a1, a2))
            }
            ([Handle, APoly, APoly, APoly], Void) => {
                let f: extern "C" fn(u64, u64, u64, u64) = std::mem::transmute(m.fn_ptr);
                f(recv_h, a0, a1, a2);
                rebox(Void, 0)
            }
            ([Handle], I64) => {
                let f: extern "C" fn(u64) -> i64 = std::mem::transmute(m.fn_ptr);
                rebox(I64, f(recv_h) as u64)
            }
            // An F64 RETURN is fine — transmuting to a correctly-typed
            // `-> f64` reads XMM0, which is where the callee put it (the
            // numeric getters: `bytesRead`, `remotePort`, `bufferSize`, …).
            ([Handle], AbiType::F64) => {
                let f: extern "C" fn(u64) -> f64 = std::mem::transmute(m.fn_ptr);
                rebox(AbiType::F64, f(recv_h).to_bits())
            }
            // ---- VALUE-CLASS (autoboxed primitive) shapes ----------------------
            // The `#[rtse::class("X", value)]` methods take the receiver as a raw
            // `PolyValue` WORD (`recv`, NOT the handle — the body unboxes it) and
            // numeric args as real `f64` (a correctly-TYPED transmute puts the
            // double in XMM, so the earlier all-integer hazard does not apply).
            ([APoly], Handle) => {
                let f: extern "C" fn(u64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv))
            }
            ([APoly], Bool) => {
                let f: extern "C" fn(u64) -> i64 = std::mem::transmute(m.fn_ptr);
                rebox(Bool, f(recv) as u64)
            }
            ([APoly], AbiType::F64) => {
                let f: extern "C" fn(u64) -> f64 = std::mem::transmute(m.fn_ptr);
                rebox(AbiType::F64, f(recv).to_bits())
            }
            ([APoly, AbiType::F64], Handle) => {
                let f: extern "C" fn(u64, f64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv, genops_num(a0)))
            }
            ([APoly, AbiType::F64], AbiType::F64) => {
                let f: extern "C" fn(u64, f64) -> f64 = std::mem::transmute(m.fn_ptr);
                rebox(AbiType::F64, f(recv, genops_num(a0)).to_bits())
            }
            ([APoly, AbiType::F64, AbiType::F64], Handle) => {
                let f: extern "C" fn(u64, f64, f64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv, genops_num(a0), genops_num(a1)))
            }
            ([APoly, StrPtr], Handle) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv, s.as_ptr(), s.len() as i64))
            }
            ([APoly, StrPtr], AbiType::F64) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64) -> f64 = std::mem::transmute(m.fn_ptr);
                rebox(AbiType::F64, f(recv, s.as_ptr(), s.len() as i64).to_bits())
            }
            ([APoly, StrPtr, AbiType::F64], AbiType::F64) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64, f64) -> f64 = std::mem::transmute(m.fn_ptr);
                rebox(AbiType::F64, f(recv, s.as_ptr(), s.len() as i64, genops_num(a1)).to_bits())
            }
            ([APoly, StrPtr, AbiType::F64], Bool) => {
                let s = owned_str(a0);
                let f: extern "C" fn(u64, *const u8, i64, f64) -> i64 = std::mem::transmute(m.fn_ptr);
                rebox(Bool, f(recv, s.as_ptr(), s.len() as i64, genops_num(a1)) as u64)
            }
            ([APoly, StrPtr, StrPtr], Handle) => {
                let s0 = owned_str(a0);
                let s1 = owned_str(a1);
                let f: extern "C" fn(u64, *const u8, i64, *const u8, i64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv, s0.as_ptr(), s0.len() as i64, s1.as_ptr(), s1.len() as i64))
            }
            ([APoly, AbiType::F64, StrPtr], Handle) => {
                let s = owned_str(a1);
                let f: extern "C" fn(u64, f64, *const u8, i64) -> u64 = std::mem::transmute(m.fn_ptr);
                rebox(Handle, f(recv, genops_num(a0), s.as_ptr(), s.len() as i64))
            }
            // An F64 ARGUMENT on a Handle-first method is deliberately absent: on
            // Win64 a double rides an XMM register, so passing it through an
            // all-integer transmute would hand the callee garbage. Such a method
            // falls back (None) — unsupported beats silently wrong. (The value-class
            // shapes above ARE safe because their transmute types the arg `f64`.)
            _ => return None,
        }
    };
    Some(out)
}
