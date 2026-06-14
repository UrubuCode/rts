//! Codegen-owned Error-family + wrapper (Boolean/Number/String) constructors,
//! Error instance props, and `instanceof` runtime tags (P5.3) — PolyValue-native.
//!
//! Like every `__rtsadp_*` surface these wrap the REAL primordial runtime symbols
//! (`globals::{error,boolean,number,string}`, reached through the `rts-runtime`
//! facade) and bridge the engine's [`PolyValue`] value model:
//!
//! - a constructed instance is a `TAG_OBJECT` PolyValue over the REAL runtime
//!   handle (an `Entry::ErrorObj` / `Entry::BooleanBox` / boxed Number/String);
//! - Error props (`.message`/`.name`/`.stack`) return string PolyValue words;
//! - `instanceof` is a real runtime tag inspection (the `Entry` kind / error
//!   name) — correct for ANY operand, not an AST guess.

use rts_runtime::namespaces::gc::handles as rt_handles;
use rts_runtime::namespaces::gc::string_pool as rt_str;
use rts_runtime::namespaces::globals::boolean as rt_bool;
use rts_runtime::namespaces::globals::error::instance as rt_err;
use rts_runtime::namespaces::globals::number as rt_num;
use rts_runtime::namespaces::globals::string::rt as rt_gl_str;

use super::abi_adapter;
use super::{genops, PolyValue};

/// Box a real runtime handle as a `TAG_OBJECT` PolyValue word (a wrapper / error
/// instance is an object: `typeof === "object"`).
fn box_object(handle: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(handle)).raw()
}

/// The real runtime handle behind a `TAG_OBJECT` instance word.
fn unbox_object(word: u64) -> u64 {
    rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(PolyValue::from_raw(word).as_handle())
}

/// `(ptr, len)` of a string PolyValue word, through the REAL pool.
fn str_ptr_len(word: u64) -> (i64, i64) {
    let handle = abi_adapter::real_handle_of(PolyValue::from_raw(word));
    let ptr = rt_str::__RTS_FN_NS_GC_STRING_PTR(handle) as i64;
    let len = rt_str::__RTS_FN_NS_GC_STRING_LEN(handle);
    (ptr, len)
}

/// Box a raw runtime string handle as a string PolyValue word.
fn box_string(handle: u64) -> u64 {
    abi_adapter::poly_from_real_handle(handle).raw()
}

// ===========================================================================
// Error-family constructors — `new Error(msg)` / `new TypeError(msg)` / …
// The message arrives as a string PolyValue word; options is always 0 (the
// `{ cause }` form is a later increment — the lowering passes no options).
// ===========================================================================

macro_rules! err_ctor {
    ($name:ident, $real:path) => {
        /// Construct an error instance from a string-message PolyValue word.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(msg_word: u64) -> u64 {
            let (ptr, len) = str_ptr_len(msg_word);
            box_object($real(ptr, len, 0))
        }
    };
}

err_ctor!(__rtsadp_err_new, rt_err::__RTS_FN_GL_ERROR_NEW);
err_ctor!(__rtsadp_err_new_type, rt_err::__RTS_FN_GL_TYPE_ERROR_NEW);
err_ctor!(__rtsadp_err_new_range, rt_err::__RTS_FN_GL_RANGE_ERROR_NEW);
err_ctor!(__rtsadp_err_new_reference, rt_err::__RTS_FN_GL_REF_ERROR_NEW);
err_ctor!(__rtsadp_err_new_syntax, rt_err::__RTS_FN_GL_SYNTAX_ERROR_NEW);
err_ctor!(__rtsadp_err_new_uri, rt_err::__RTS_FN_GL_URI_ERROR_NEW);
err_ctor!(__rtsadp_err_new_eval, rt_err::__RTS_FN_GL_EVAL_ERROR_NEW);

/// `e.message` — a string PolyValue word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_err_message(err_word: u64) -> u64 {
    box_string(rt_err::__RTS_FN_GL_ERROR_MESSAGE(unbox_object(err_word)))
}

/// `e.name` — a string PolyValue word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_err_name(err_word: u64) -> u64 {
    box_string(rt_err::__RTS_FN_GL_ERROR_NAME(unbox_object(err_word)))
}

/// `e.stack` — a string PolyValue word (basic: `"<name>: <message>"`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_err_stack(err_word: u64) -> u64 {
    box_string(rt_err::__RTS_FN_GL_ERROR_STACK(unbox_object(err_word)))
}

/// `e.toString()` — `"<name>: <message>"`, a string PolyValue word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_err_to_string(err_word: u64) -> u64 {
    box_string(rt_err::__RTS_FN_GL_ERROR_TO_STRING(unbox_object(err_word)))
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

// ===========================================================================
// `instanceof` runtime tags — inspect the instance's real `Entry` kind / error
// name. A non-object / wrong-kind operand yields `false` (never a wrong true).
// ===========================================================================

/// `x instanceof Error` (any error subtype) — true iff `x` is a `TAG_OBJECT` over
/// a runtime `Entry::ErrorObj` (or a user Map carrying name+message — a subclass
/// instance built via `super(msg)`). Reuses the runtime's own `IS_ERROR` tag.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_is_error(word: u64) -> u64 {
    let v = PolyValue::from_raw(word);
    let yes = v.is_object() && rt_err::__RTS_FN_GL_IS_ERROR(unbox_object(word)) != 0;
    PolyValue::bool(yes).raw()
}
