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
use rts_runtime::namespaces::globals::string::rt as rt_gl_str;

use super::abi_adapter;
use super::{PolyValue, genops};

/// Box a real runtime handle as a `TAG_OBJECT` PolyValue word (a wrapper instance
/// is an object: `typeof === "object"`).
fn box_object(handle: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(handle)).raw()
}

// ===========================================================================
// Wrapper objects — `new String(x)`. JS quirk: this is an OBJECT
// (`typeof new String("x") === "object"`).
//
// `new Boolean(x)` and `new Number(x)` are no longer codegen wrapper trampolines:
// their `.ts` `class Boolean`/`class Number` (boolean.ts / number.ts) now OWN
// construction (a shape-based object via the user-class path) — see
// `front/run/globalclass::is_wrapper_primordial`. The former
// `__rtsadp_w_{boolean,number}_new` were deleted with that migration. (The Rust
// `__RTS_FN_GL_{BOOLEAN,NUMBER}_*` externs stay — the frozen old engine uses them.)
// ===========================================================================

/// `new String(x)` — boxes the ToString of `x` (a real string handle) as a String
/// object. `STRING_NEW_BOXED` takes the underlying string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_w_string_new(value_word: u64) -> u64 {
    let s_word = genops::__rtsadp_to_string(value_word);
    let handle = abi_adapter::real_handle_of(PolyValue::from_raw(s_word));
    box_object(rt_gl_str::__RTS_FN_GL_STRING_NEW_BOXED(handle))
}
