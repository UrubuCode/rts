//! The ABI adapter — the ONE legitimately-new runtime-facing codegen code.
//!
//! It bridges the new engine's [`PolyValue`] value model to the REAL RTS runtime
//! (`rts-engine`/`rts-primitives`/`rts-shared`/`rts-std`, reached through the
//! `rts-runtime` facade). It defines **no** `__RTS_FN_*` symbol — those belong to
//! the runtime and are consumed as-is. It owns only:
//!
//! 1. **The PolyValue ↔ handle bridge.** A real runtime handle is a `u64 =
//!    [63..48]generation(16) | [47..5]slot(43) | [4..0]shard(5)`, and the engine's
//!    `with_entry`/`free_handle`/`get` VALIDATE the 16-bit generation. A
//!    [`PolyValue`] payload is only 48 bits (slot+shard) and cannot carry the
//!    generation. Rather than a side table, the payload stores the bare 48-bit
//!    slot+shard and the generation is reconstructed on demand from the slot's own
//!    live generation by the REAL engine symbols
//!    `__RTS_FN_NS_GC_POLY_FROM_HANDLE` (box: drop the gen) /
//!    `__RTS_FN_NS_GC_POLY_TO_HANDLE` (unbox: reconstruct the gen). The
//!    string/object BYTES live in the REAL pool; the PolyValue carries the slot.
//!
//! 2. **The generic-operator trampolines** (`__rtsadp_*`, codegen-owned, NOT
//!    `__RTS_FN_*`): `add`/`strict_eq`/`typeof`/`to_string`/`to_boolean` +
//!    `print_line`. These allocate no JS heap of their own; their heap-touching
//!    parts call the REAL string pool. The JIT installs them by symbol just like
//!    the real runtime symbols. (The old `__rtsadp_store`/`__rtsadp_load` table
//!    trampolines are gone — replaced by the engine POLY bridge above.)
//!
//! 3. **Host helpers** ([`intern_poly`], [`resolve_poly`]) so the host (tests,
//!    `console.log` lowering, the P1 proofs) can box/read strings through the
//!    SAME real pool the JIT code uses.
//!
//! ## GC — no side table, no GC-root hole
//!
//! The PolyValue payload is the bare HandleTable slot+shard, so a boxed handle is
//! a normal GC reference: the conservative scanner finds the NaN-boxed word on the
//! stack / in `Entry::Vec`/`Map` children and `mark_handle` normalizes it
//! (`crate::heap::poly::poly_handle_normalize` in `rts-engine`) to the underlying
//! slot before marking. There is no process-global table holding the only strong
//! ref, hence no GC-root hole to document.

use std::cell::RefCell;

use rts_runtime::namespaces::gc::handles as rt_handles;
use rts_runtime::namespaces::gc::string_pool as rt_str;
use rts_runtime::namespaces::io as rt_io;

use super::PolyValue;

// ===========================================================================
// Host helpers — box/read a string through the REAL string pool.
// ===========================================================================

/// Intern `s` in the REAL runtime string pool and box the result as a string
/// `PolyValue` whose payload is the real handle's 48-bit slot+shard (the
/// generation is reconstructed on demand by `POLY_TO_HANDLE`).
pub fn intern_poly(s: &str) -> PolyValue {
    // STRING_NEW reads `len` bytes from `ptr` internally; the extern is a safe
    // `extern "C" fn`, and we pass a live &str's ptr+len.
    let handle = rt_str::__RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64);
    // PIN as a permanent GC root: the JIT splices this handle in as a code
    // `iconst` immediate (string literals are interned ONCE at lowering time),
    // so it never appears on a scanned stack/cell/global. Without pinning the
    // GC sweeps these live constants and the immediate later reads a recycled
    // slot → corrupted string. The constant lives for the whole program, which
    // is exactly a pinned root's lifetime.
    rt_handles::__RTS_FN_NS_GC_PIN_HANDLE(handle);
    poly_from_real_handle(handle)
}

/// Box an already-allocated real string handle as a string `PolyValue`, storing
/// its bare 48-bit slot+shard payload (no side table).
pub fn poly_from_real_handle(handle: u64) -> PolyValue {
    PolyValue::from_str_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(handle))
}

/// The real runtime string handle behind a string `PolyValue`, reconstructing
/// the generation from the live slot.
pub fn real_handle_of(v: PolyValue) -> u64 {
    debug_assert!(v.is_string(), "real_handle_of on a non-string PolyValue");
    rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle())
}

/// Read a string `PolyValue` back to its UTF-8 text, via the REAL pool
/// (STRING_PTR + STRING_LEN). Debug-asserts the value is a string.
pub fn resolve_poly(v: PolyValue) -> String {
    debug_assert!(v.is_string(), "resolve_poly on a non-string PolyValue");
    real_handle_to_string(real_handle_of(v))
}

/// Read a real runtime string handle's bytes into an owned `String`.
pub fn real_handle_to_string(handle: u64) -> String {
    let ptr = rt_str::__RTS_FN_NS_GC_STRING_PTR(handle);
    let len = rt_str::__RTS_FN_NS_GC_STRING_LEN(handle);
    bytes_to_string(ptr, len)
}

/// Read `(ptr, len)` UTF-8 bytes into an owned `String` (empty on null/negative).
fn bytes_to_string(ptr: *const u8, len: i64) -> String {
    if ptr.is_null() || len < 0 {
        return String::new();
    }
    // SAFETY: the pool guarantees `len` valid UTF-8 bytes at `ptr` while the
    // handle is live (it is — the table holds a strong ref).
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    String::from_utf8_lossy(bytes).into_owned()
}

// ===========================================================================
// console.log line sink — the REAL IO_PRINT, or a test capture buffer.
// ===========================================================================
//
// rts-std's io layer has NO stdout-redirect hook (verified): IO_PRINT writes
// straight to `std::io::stdout()`. So in-process unit tests cannot read back
// what a JIT'd program printed. We give the console.log lowering ONE marshaling
// trampoline — [`__rtsadp_print_line`] — taking the final joined string as a
// real `(ptr, len)` (the StrPtr 2-slot ABI, computed in IR via STRING_PTR/LEN):
//
//   * normal run: forward verbatim to the REAL `__RTS_FN_NS_IO_PRINT(ptr, len)`
//     (which appends the trailing newline). The bun fixture harness validates
//     true end-to-end stdout against `bun`.
//   * capture mode (tests only): append `line + "\n"` to a thread-local buffer
//     instead of touching stdout, so a unit test reads the exact rendered output
//     while STILL exercising the real string pool (NEW/CONCAT/PTR/LEN) that
//     produced `(ptr, len)`.

thread_local! {
    /// `Some` while a render-capture is active; the JIT'd program's console.log
    /// lines land here instead of stdout. `None` = print to real stdout.
    static CAPTURE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Run `f` with console.log output captured, returning what it "printed" (each
/// line terminated by `"\n"`). The capture is thread-local, so concurrent test
/// runs on different threads do not interleave. Re-entrancy is not supported
/// (nested captures would clobber); the harness never nests.
pub fn with_capture<R>(f: impl FnOnce() -> R) -> (R, String) {
    CAPTURE.with(|c| *c.borrow_mut() = Some(String::new()));
    let r = f();
    let out = CAPTURE.with(|c| c.borrow_mut().take().unwrap_or_default());
    (r, out)
}

/// `console.*` line marshaling trampoline: `(ptr, len)` of the final joined
/// line + a `to_stderr` flag selecting the sink. In capture mode appends to the
/// thread-local buffer (both streams share the one capture — the unit tests read
/// rendered text regardless of stream, matching bun's combined harness output);
/// otherwise forwards to the REAL `__RTS_FN_NS_IO_PRINT` (stdout, `to_stderr==0`)
/// or `__RTS_FN_NS_IO_EPRINT` (stderr, `to_stderr!=0`) — the same symbol split
/// the `console` Registry namespace declares per method (`log`→PRINT,
/// `warn`/`error`→EPRINT). The newline is appended by the runtime.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_print_line(ptr: *const u8, len: i64, to_stderr: i64) {
    let capturing = CAPTURE.with(|c| c.borrow().is_some());
    if capturing {
        let line = bytes_to_string(ptr, len);
        CAPTURE.with(|c| {
            if let Some(buf) = c.borrow_mut().as_mut() {
                buf.push_str(&line);
                buf.push('\n');
            }
        });
    } else if to_stderr != 0 {
        // Forward to the REAL io eprint (stderr; appends the newline itself).
        rt_io::__RTS_FN_NS_IO_EPRINT(ptr, len);
    } else {
        // Forward to the REAL io print (stdout; appends the newline itself).
        rt_io::__RTS_FN_NS_IO_PRINT(ptr, len);
    }
}
