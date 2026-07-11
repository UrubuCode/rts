//! node:process — value helpers (intern, object/array builders, throw) and the
//! Node platform/arch name mapping + the process-lifetime clock base.

use std::sync::OnceLock;
use std::time::Instant;

use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::{alloc_shaped_object, string_word};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

pub fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

pub fn str_word(s: &str) -> i64 {
    string_word(s.as_bytes()) as i64
}

pub fn num_word(v: f64) -> i64 {
    v.to_bits() as i64
}

/// A shaped object → raw handle.
pub fn object(keys: &[&str], values: &[i64]) -> u64 {
    alloc_shaped_object(keys, values)
}

/// An array → raw handle.
pub fn array(words: Vec<i64>) -> u64 {
    alloc_entry(Entry::Vec(Box::new(words)))
}

pub fn throw(kind: &str, message: &str) {
    unsafe {
        __rtsadp_throw_js_error(kind.as_ptr(), kind.len() as i64, message.as_ptr(), message.len() as i64);
    }
}

/// Node's `process.platform` for `std::env::consts::OS`.
pub fn node_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    }
}

/// Node's `process.arch` for `std::env::consts::ARCH`.
pub fn node_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        "powerpc64" => "ppc64",
        "s390x" => "s390x",
        other => other,
    }
}

/// The process-lifetime clock base (first-access `Instant`). `uptime()` and
/// `hrtime()` measure from here — a high-resolution monotonic origin.
pub fn clock_base() -> Instant {
    static BASE: OnceLock<Instant> = OnceLock::new();
    *BASE.get_or_init(Instant::now)
}
