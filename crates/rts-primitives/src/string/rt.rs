//! Shared string-pool alloc helper for the `String` primordial.
//!
//! The ENTIRE `String` surface — every instance method, the `new String(x)`
//! wrapper, AND the `fromCharCode`/`fromCodePoint` statics — is now computed in
//! pure Rust via [`super::strops`] (the value-class + the `__rtsadp_str_*`
//! variadic-static trampolines both call `strops`), so NO `__RTS_FN_GL_STRING_*`
//! extern remains. `alloc_str` stays as the shared pool-alloc helper used by
//! [`super::search`] (the `rts:string` namespace regex family).

use crate::gc_surface::__RTS_FN_NS_GC_STRING_NEW;

pub(crate) fn alloc_str(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}
