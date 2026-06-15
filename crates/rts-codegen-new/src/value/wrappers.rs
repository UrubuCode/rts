//! Codegen-owned wrapper (Boolean/Number/String) constructors — PolyValue-native.
//!
//! Like every `__rtsadp_*` surface these wrap the REAL primordial runtime symbols
//! (`globals::{boolean,number,string}`, reached through the `rts-runtime` facade)
//! and bridge the engine's [`PolyValue`] value model: a constructed wrapper is a
//! `TAG_OBJECT` PolyValue over the REAL runtime handle (`typeof === "object"`).
//!
//! NOTE: the Error FAMILY used to live here too (ctors + `.message`/`.name`/
//! `.stack`/`toString` props + `__rtsadp_is_error`). It is now a PRIMORDIAL `.ts`
//! class (`rts-primitives/src/error.ts`, engine prelude) constructed as a shape-
//! based object through the normal user-class path, so those trampolines were
//! deleted. (The Rust `globals::error` runtime + `__RTS_FN_GL_ERROR_*` externs stay
//! — the FROZEN old engine `rts-codegen-old` still uses them.)

use rts_runtime::namespaces::gc::handles as rt_handles;
use rts_runtime::namespaces::globals::boolean as rt_bool;
use rts_runtime::namespaces::globals::number as rt_num;
use rts_runtime::namespaces::globals::string::rt as rt_gl_str;

use super::abi_adapter;
use super::{PolyValue, genops};

/// Box a real runtime handle as a `TAG_OBJECT` PolyValue word (a wrapper instance
/// is an object: `typeof === "object"`).
fn box_object(handle: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(handle)).raw()
}

// ===========================================================================
// Wrapper objects — `new Boolean(x)` / `new Number(x)` / `new String(x)`. JS
// quirk: these are OBJECTS (`typeof new Number(5) === "object"`).
// ===========================================================================

/// `new Boolean(x)` — boxes the ToBoolean of `x` as a Boolean object.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_w_boolean_new(value_word: u64) -> u64 {
    let b = genops::to_boolean(PolyValue::from_raw(value_word));
    box_object(rt_bool::__RTS_FN_GL_BOOLEAN_NEW(b as i64))
}

/// `new Number(x)` — boxes the ToNumber of `x` as a Number object.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_w_number_new(value_word: u64) -> u64 {
    let n = genops::to_number(PolyValue::from_raw(value_word));
    box_object(rt_num::__RTS_FN_GL_NUMBER_NEW_BOXED(n))
}

/// `new String(x)` — boxes the ToString of `x` (a real string handle) as a String
/// object. `STRING_NEW_BOXED` takes the underlying string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_w_string_new(value_word: u64) -> u64 {
    let s_word = genops::__rtsadp_to_string(value_word);
    let handle = abi_adapter::real_handle_of(PolyValue::from_raw(s_word));
    box_object(rt_gl_str::__RTS_FN_GL_STRING_NEW_BOXED(handle))
}
