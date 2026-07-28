//! node:os — shared value-word builders + platform-name normalization.
//!
//! Every `node:os` native fn returns either a GC string handle (`intern`), a
//! native scalar, or a shaped object / array built from PolyValue **words**.
//! These helpers wrap the engine's object/array primitives
//! (`alloc_shaped_object`, `Entry::Vec`, `string_word`, `handle_word_auto`) so
//! no module duplicates the value-model encoding — objects and arrays are the
//! genuine engine representation, reachable from user JS as ordinary
//! objects/arrays.

use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::{
    alloc_shaped_object, bool_word, handle_word_auto, null_word, string_word,
};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    /// Runtime-layer throw bridge (rts-runtime): sets the engine pending-error
    /// slot with a real `kind` Error instance. Paired with `MemberFlags::THROWS`
    /// on the member so the front emits the post-call unwind check.
    fn __rtsadp_throw_js_error(
        kind_ptr: *const u8,
        kind_len: i64,
        msg_ptr: *const u8,
        msg_len: i64,
    );
}

/// Throw a JS `kind` Error with `message` (via the engine pending-error slot).
/// Only meaningful from a `MemberFlags::THROWS`-flagged member.
pub fn throw_error(kind: &str, message: &str) {
    unsafe {
        __rtsadp_throw_js_error(
            kind.as_ptr(),
            kind.len() as i64,
            message.as_ptr(),
            message.len() as i64,
        );
    }
}

/// Interns a Rust string as a GC string handle (the ABI `Handle` return of a
/// top-level string-valued `os.*` function).
pub fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// `std::env::var(key)` or `""` when unset — the env-fallback identity source
/// (`HOME`/`USERPROFILE`/`SHELL`/`USER`/…) used across the module.
pub fn env_or_empty(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

// --- PolyValue words for object/array element slots -------------------------

/// A JS `number` word carrying `v` (all numbers are IEEE-754 doubles; the raw
/// f64 bit-pattern of any finite value is a valid inline-float PolyValue word).
pub fn num_word(v: f64) -> i64 {
    v.to_bits() as i64
}

/// A `string` element word.
pub fn str_word(s: &str) -> i64 {
    string_word(s.as_bytes()) as i64
}

/// A `boolean` element word.
pub fn bool_w(b: bool) -> i64 {
    bool_word(b) as i64
}

/// The `null` element word.
pub fn null_w() -> i64 {
    null_word() as i64
}

/// Build a shaped object `{ keys[i]: values[i] }` and return its raw handle
/// (the ABI `Handle` return of an object-valued top-level `os.*` function).
pub fn object(keys: &[&str], values: &[i64]) -> u64 {
    alloc_shaped_object(keys, values)
}

/// Build a shaped object and return it as an OBJECT element **word** (for
/// nesting inside another object/array slot).
pub fn object_word(keys: &[&str], values: &[i64]) -> i64 {
    handle_word_auto(object(keys, values)) as i64
}

/// Allocate an array from element `words` and return its raw handle (the ABI
/// `Handle` return of an array-valued top-level `os.*` function).
pub fn array(words: Vec<i64>) -> u64 {
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// Allocate an array and return it as an ARRAY element **word** (for nesting).
pub fn array_word(words: Vec<i64>) -> i64 {
    handle_word_auto(array(words)) as i64
}

// --- Node platform / arch / type normalization ------------------------------

/// Node's `os.platform()` name for `std::env::consts::OS`: `"windows"` →
/// `"win32"`, `"macos"` → `"darwin"`, everything else passes through unchanged
/// (`"linux"`, `"freebsd"`, `"openbsd"`, `"android"`, `"ios"`, …).
pub fn node_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    }
}

/// Node's `os.arch()` name for `std::env::consts::ARCH`.
pub fn node_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        "powerpc64" => "ppc64",
        "powerpc" => "ppc",
        "riscv64" => "riscv64",
        "s390x" => "s390x",
        "mips" => "mips",
        "mips64" => "mips64",
        "loongarch64" => "loong64",
        other => other,
    }
}

/// Node's `os.type()` — `uname -s`-style OS name. Uses the real `uname(3)`
/// `sysname` on POSIX (see [`crate::os::sys`]); Windows is always
/// `"Windows_NT"`.
pub fn node_type() -> String {
    #[cfg(windows)]
    {
        "Windows_NT".to_string()
    }
    #[cfg(unix)]
    {
        crate::os::sys::uname_field(crate::os::sys::UnameField::Sysname)
            .unwrap_or_else(|| fallback_type())
    }
}

/// Best-effort capitalized `std::env::consts::OS` when `uname` is unavailable.
#[cfg(unix)]
fn fallback_type() -> String {
    let os = std::env::consts::OS;
    let mut chars = os.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
