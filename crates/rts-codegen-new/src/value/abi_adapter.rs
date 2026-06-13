//! The ABI adapter — the ONE legitimately-new runtime-facing codegen code.
//!
//! It bridges the new engine's [`PolyValue`] value model to the REAL RTS runtime
//! (`rts-engine`/`rts-primitives`/`rts-shared`/`rts-std`, reached through the
//! `rts-runtime` facade). It defines **no** `__RTS_FN_*` symbol — those belong to
//! the runtime and are consumed as-is. It owns only:
//!
//! 1. **The handle indirection table** (the crux). A real runtime handle is a
//!    `u64 = [63..48]generation(16) | [47..5]slot(43) | [4..0]shard(5)`, and the
//!    engine's `with_entry`/`free_handle`/`get` VALIDATE the 16-bit generation. A
//!    [`PolyValue`] payload is only 48 bits (slot+shard) and CANNOT carry the
//!    generation — so a real handle cannot be boxed directly into a PolyValue and
//!    handed back. Instead we keep a process-global append-only table mapping a
//!    small `idx` (the 48-bit PolyValue payload) → the full real `u64` handle.
//!    The string/object BYTES live in the REAL pool; the PolyValue carries the
//!    `idx`. Unbox = `table[idx]` → real handle → pass to `__RTS_FN_*`.
//!
//! 2. **The adapter trampolines** (`__rtsadp_*`, codegen-owned, NOT `__RTS_FN_*`).
//!    These allocate no JS heap; they only index the table or run a generic JS
//!    operator whose heap-touching parts call the REAL string pool. The JIT
//!    installs them by symbol just like the real runtime symbols.
//!
//! 3. **Host helpers** ([`intern_poly`], [`resolve_poly`]) so the host (tests,
//!    `console.log` lowering, the P1 proofs) can box/read strings through the
//!    SAME real pool the JIT code uses.
//!
//! ## GC NOTE — known limitation (P1)
//!
//! TODO(gc-root): register the adapter handle table as a GC root or free each
//! real handle on PolyValue death. The table currently holds the only strong ref
//! to those runtime handles, but it is NOT a registered GC root, so the runtime
//! GC (256-alloc tick mark+sweep) could sweep a handle whose only owner is this
//! table. For P1 this is acceptable ONLY because the programs the new engine runs
//! are tiny (well under the GC tick); it is NOT a silent omission — it is a loud,
//! documented follow-up.

use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};

use rts_runtime::namespaces::gc::string_pool as rt_str;
use rts_runtime::namespaces::io as rt_io;

use super::PolyValue;

// ===========================================================================
// (1) Handle indirection table: PolyValue 48-bit idx  →  real u64 handle.
// ===========================================================================

/// The append-only `idx → real_handle` table. Append-only for P1 (no reuse / no
/// free), so an `idx` once handed out stays valid for the process lifetime.
fn table() -> &'static Mutex<Vec<u64>> {
    static TABLE: OnceLock<Mutex<Vec<u64>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Store a full real runtime handle, returning the small `idx` that a PolyValue
/// payload can carry (≤ 48 bits). Host-callable; the JIT reaches the same logic
/// via [`__rtsadp_store`].
pub fn store(full_handle: u64) -> u64 {
    let mut guard = table().lock().expect("adapter table poisoned");
    let idx = guard.len() as u64;
    debug_assert!(idx <= super::PAYLOAD_MASK, "adapter table idx exceeds 48 bits");
    guard.push(full_handle);
    idx
}

/// Resolve an `idx` back to the full real runtime handle. Panics on an
/// out-of-range idx (a bug: a heap PolyValue must always point at a live slot).
pub fn load(idx: u64) -> u64 {
    let guard = table().lock().expect("adapter table poisoned");
    *guard
        .get(idx as usize)
        .unwrap_or_else(|| panic!("adapter table idx {idx} out of range (len {})", guard.len()))
}

// ---------------------------------------------------------------------------
// Extern "C" trampolines the JIT calls for the table ops. Codegen-owned
// (`__rtsadp_*`), allocate no JS heap, only index the Vec.
// ---------------------------------------------------------------------------

/// JIT entry for [`store`]: full real handle → PolyValue idx.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_store(full_handle: u64) -> u64 {
    store(full_handle)
}

/// JIT entry for [`load`]: PolyValue idx → full real handle.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_load(idx: u64) -> u64 {
    load(idx)
}

// ===========================================================================
// Host helpers — box/read a string through the REAL string pool.
// ===========================================================================

/// Intern `s` in the REAL runtime string pool and box the result as a string
/// `PolyValue` whose payload is the table idx of the real handle.
pub fn intern_poly(s: &str) -> PolyValue {
    // STRING_NEW reads `len` bytes from `ptr` internally; the extern is a safe
    // `extern "C" fn`, and we pass a live &str's ptr+len.
    let handle = rt_str::__RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64);
    PolyValue::from_str_handle(store(handle))
}

/// Box an already-allocated real string handle as a string `PolyValue`.
pub fn poly_from_real_handle(handle: u64) -> PolyValue {
    PolyValue::from_str_handle(store(handle))
}

/// The real runtime string handle behind a string `PolyValue`.
pub fn real_handle_of(v: PolyValue) -> u64 {
    debug_assert!(v.is_string(), "real_handle_of on a non-string PolyValue");
    load(v.as_handle())
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

/// `console.log` line marshaling trampoline: `(ptr, len)` of the final joined
/// line. In capture mode appends to the thread-local buffer; otherwise forwards
/// to the REAL `__RTS_FN_NS_IO_PRINT` (newline appended by the runtime).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_print_line(ptr: *const u8, len: i64) {
    let capturing = CAPTURE.with(|c| c.borrow().is_some());
    if capturing {
        let line = bytes_to_string(ptr, len);
        CAPTURE.with(|c| {
            if let Some(buf) = c.borrow_mut().as_mut() {
                buf.push_str(&line);
                buf.push('\n');
            }
        });
    } else {
        // Forward to the REAL io print (appends the newline itself).
        rt_io::__RTS_FN_NS_IO_PRINT(ptr, len);
    }
}
