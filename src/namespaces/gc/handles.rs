//! Handle store backed by gc-arena.
//!
//! All runtime-managed values are stored as `Gc<'gc, RefLock<Entry>>`
//! inside a single gc-arena `Arena`. Handles exposed to the ABI are
//! opaque `u64` values that encode a `slotmap::KeyData` (version +
//! slot index), providing use-after-free protection equivalent to the
//! old generation-tagged slab.
//!
//! # Thread safety
//!
//! gc-arena's `Arena` is not `Send` (uses `Rc` internally for pacing
//! metrics). We wrap it in `GcArenaWrapper` which implements `Send` via
//! `unsafe impl`: the invariant is that the arena is *only* ever accessed
//! under the global `Mutex<GcArenaWrapper>`, so the inner `Rc` is never
//! touched from multiple threads simultaneously.
//!
//! Phase 3 will replace this with per-thread arenas for parallel workloads.
//!
//! # Collection
//!
//! - `free_handle`: calls `cleanup_entry` synchronously (network shutdown,
//!   process try_wait), then removes the `Gc` from `HandleRoot.slots`.
//!   Once unrooted, the arena can sweep the memory on the next cycle.
//! - `alloc_entry`: calls `arena.collect_debt()` after every allocation so
//!   the incremental collector keeps pace with the allocation rate.
//! - `collect_debt()` / `finish_cycle()`: public entry points for explicit
//!   collection at quiescence points or user-triggered GC.

use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicUsize, Ordering};

use gc_arena::{Arena, Collect, Gc, Rootable};
use gc_arena::lock::RefLock;
use slotmap::{SlotMap, new_key_type};

// ─── Public supporting types ──────────────────────────────────────────────────

/// Value kinds stored behind a handle.
#[derive(Debug)]
pub enum Entry {
    /// UTF-8 string owned on the heap.
    String(Vec<u8>),
    /// Fixed-point decimal number (`bigfloat` namespace).
    BigFixed(Box<super::super::bigfloat::fixed::FixedDecimal>),
    /// Raw byte buffer (`buffer` namespace).
    Buffer(Vec<u8>),
    /// Child process (`process` namespace).
    ProcessChild(Box<std::process::Child>),
    /// IndexMap<String, i64> (`collections` namespace). IndexMap preserves
    /// insertion order for JS integer-key + string-key enumeration semantics.
    Map(Box<indexmap::IndexMap<String, i64>>),
    /// Vec<i64> (`collections` namespace).
    Vec(Box<Vec<i64>>),
    /// Compiled regex (`regex` namespace).
    Regex(Box<regex::Regex>),
    /// CString (`ffi` namespace).
    CString(Box<std::ffi::CString>),
    /// OsString (`ffi` namespace).
    OsString(Box<std::ffi::OsString>),
    /// AtomicI64 (`atomic` namespace).
    AtomicI64(Box<std::sync::atomic::AtomicI64>),
    /// AtomicBool (`atomic` namespace).
    AtomicBool(Box<std::sync::atomic::AtomicBool>),
    /// AtomicU64 backing an f64 via bit-transmute (`atomic` namespace).
    AtomicF64(Box<std::sync::atomic::AtomicU64>),
    /// Arc<Mutex<i64>> (`sync` namespace). Arc allows the guard stored in
    /// GUARDS thread-local to outlive the handle (#280).
    SyncMutex(std::sync::Arc<std::sync::Mutex<i64>>),
    /// Arc<RwLock<i64>> (`sync` namespace). Same Arc reasoning as SyncMutex.
    SyncRwLock(std::sync::Arc<std::sync::RwLock<i64>>),
    /// std::sync::Once (`sync` namespace).
    SyncOnce(Box<std::sync::Once>),
    /// TCP listener (`net` namespace).
    TcpListener(Box<std::net::TcpListener>),
    /// TCP stream (`net` namespace).
    TcpStream(Box<std::net::TcpStream>),
    /// UDP socket + last peer (`net` namespace).
    UdpSocket(Box<UdpEntry>),
    /// TLS client stream (`tls` namespace).
    TlsClient(Box<super::super::tls::client::TlsClientStream>),
    /// Thread join handle (`thread` namespace). Consumed by join/detach.
    JoinHandle(Box<std::thread::JoinHandle<u64>>),
    /// Closure environment record (`gc` namespace).
    Env(Vec<i64>),
    /// JSON value (`json` namespace).
    Json(Box<serde_json::Value>),
    /// Native class instance (`gc` namespace).
    Instance(Box<Instance>),
    /// Date as milliseconds since Unix epoch (`date`/`globals::date`).
    DateMs(i64),
    /// Error object (`globals::error`).
    ErrorObj { message: String, name: String },
    /// EventEmitter (`globals::events`). Arc so the inner mutex can be held
    /// independently of the arena lock.
    EventEmitter(std::sync::Arc<std::sync::Mutex<dyn std::any::Any + Send>>),
    /// Primitive events emitter (`events` namespace).
    RtsEventsEmitter(Box<RtsEventsEmitter>),
    /// Synchronous resolved promise (`globals`).
    Promise(i64),
    /// HTTP fetch response (`globals::fetch`).
    HttpResponse(Box<HttpResponseData>),
}

// gc-arena constraint: Collect<'gc> and no explicit `Drop` impl.
// Entry has no Drop impl (Rust drops fields automatically).
// All inner types are 'static — no Gc<'gc, T> pointers inside —
// so NEEDS_TRACE = false (Phase 1; Phase 2 will add exact tracing
// for Map/Vec/Env/Instance fields that store handle-valued i64s).
unsafe impl<'gc> Collect<'gc> for Entry {
    const NEEDS_TRACE: bool = false;
}

#[derive(Debug)]
pub struct HttpResponseData {
    pub status: u16,
    pub url: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct RtsEventsEmitter {
    pub listeners: std::collections::HashMap<String, Vec<u64>>,
}

#[derive(Debug)]
pub struct UdpEntry {
    pub socket: std::net::UdpSocket,
    pub last_peer: Option<std::net::SocketAddr>,
}

/// Native class instance with byte-layout fields.
#[derive(Debug)]
pub struct Instance {
    pub class: u64,
    pub bytes: Vec<u8>,
}

/// Explicit cleanup for OS resources before an entry is unrooted.
/// Called synchronously by `free_handle` so peers get timely shutdown
/// and zombie processes are reaped without waiting for a GC cycle.
pub(crate) fn cleanup_entry(entry: &mut Entry) {
    match entry {
        Entry::ProcessChild(child) => {
            let _ = child.try_wait();
        }
        Entry::TcpStream(stream) => {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        Entry::TlsClient(tls) => {
            let _ = tls.tcp.shutdown(std::net::Shutdown::Both);
        }
        _ => {}
    }
}

// ─── gc-arena internals ───────────────────────────────────────────────────────

new_key_type! { struct HandleKey; }

/// Arena root: owns all live entries as `Gc<'gc, RefLock<Entry>>`.
/// Removing a key from `slots` makes the Gc unreachable; gc-arena sweeps
/// and drops it on the next collection cycle.
#[derive(Collect)]
#[collect(no_drop)]
struct HandleRoot<'gc> {
    slots: SlotMap<HandleKey, Gc<'gc, RefLock<Entry>>>,
}

/// Newtype wrapper so we can implement `Send` on a non-Send `Arena`.
struct GcArenaWrapper(Arena<gc_arena::Rootable![HandleRoot<'_>]>);

// SAFETY: `Arena` is `!Send` because of `Rc<MetricsInner>` used for
// incremental pacing. However, the `Rc` is never cloned out of the arena
// and is only accessed while the global `Mutex` below is held, ensuring
// exclusive single-threaded access at any given time.
unsafe impl Send for GcArenaWrapper {}

static GC_ARENA: OnceLock<Mutex<GcArenaWrapper>> = OnceLock::new();

fn gc_arena() -> &'static Mutex<GcArenaWrapper> {
    GC_ARENA.get_or_init(|| {
        Mutex::new(GcArenaWrapper(Arena::new(|_mc| HandleRoot {
            slots: SlotMap::with_key(),
        })))
    })
}

// ─── Live-handle cap ─────────────────────────────────────────────────────────

/// Cross-thread count of live handles. Decremented on free.
pub(crate) static LIVE_HANDLES: AtomicUsize = AtomicUsize::new(0);

/// Hard cap. 5M handles × worst-case ~1 KB each ≈ 5 GB. Programs that
/// allocate in unbounded loops without freeing hit this instead of OOM.
const HANDLES_MAX: usize = 5_000_000;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Allocates `entry` and returns an opaque handle. Drives incremental
/// GC after every allocation so collection keeps pace with allocation.
pub fn alloc_entry(entry: Entry) -> u64 {
    let prev = LIVE_HANDLES.fetch_add(1, Ordering::Relaxed);
    if prev >= HANDLES_MAX {
        LIVE_HANDLES.fetch_sub(1, Ordering::Relaxed);
        eprintln!(
            "RTS runtime: handle table exceeded limit of {HANDLES_MAX} live handles; \
             aborting (unbounded allocation loop without gc.collect or gc.free)"
        );
        std::process::abort();
    }
    let mut guard = gc_arena().lock().unwrap_or_else(|e| e.into_inner());
    let handle = guard.0.mutate_root(|mc, root| {
        let gc = Gc::new(mc, RefLock::new(entry));
        root.slots.insert(gc).data().as_ffi()
    });
    // Incremental pacing: pay off GC debt so the collector keeps up.
    guard.0.collect_debt();
    handle
}

/// Frees a handle. Calls `cleanup_entry` synchronously for network/process
/// resources, then removes the `Gc` from the root so gc-arena can sweep it.
/// Returns `false` for already-freed or invalid handles.
pub fn free_handle(handle: u64) -> bool {
    if handle == 0 {
        return false;
    }
    let key = HandleKey::from(slotmap::KeyData::from_ffi(handle));
    let mut guard = gc_arena().lock().unwrap_or_else(|e| e.into_inner());
    let removed = guard.0.mutate_root(|mc, root| {
        if let Some(&gc) = root.slots.get(key) {
            // Explicit cleanup before unrooting so OS resources are
            // released immediately rather than on the next GC cycle.
            {
                let mut entry = gc.borrow_mut(mc);
                cleanup_entry(&mut *entry);
            }
            root.slots.remove(key);
            true
        } else {
            false
        }
    });
    if removed {
        LIVE_HANDLES.fetch_sub(1, Ordering::Relaxed);
    }
    removed
}

/// Immutable access to an entry. `f` receives `None` for invalid handles.
///
/// **Do not call any handle operations inside `f`** — the global arena
/// lock is held for the duration of the call. Nested handle ops deadlock.
pub fn with_entry<R>(handle: u64, f: impl FnOnce(Option<&Entry>) -> R) -> R {
    if handle == 0 {
        return f(None);
    }
    let key = HandleKey::from(slotmap::KeyData::from_ffi(handle));
    let guard = gc_arena().lock().unwrap_or_else(|e| e.into_inner());
    guard.0.mutate(|_mc, root| match root.slots.get(key) {
        Some(&gc) => {
            let borrowed = gc.borrow();
            f(Some(&*borrowed))
        }
        None => f(None),
    })
}

/// Mutable access to an entry. `f` receives `None` for invalid handles.
///
/// **Do not call any handle operations inside `f`** — the global arena
/// lock is held for the duration of the call. Nested handle ops deadlock.
pub fn with_entry_mut<R>(handle: u64, f: impl FnOnce(Option<&mut Entry>) -> R) -> R {
    if handle == 0 {
        return f(None);
    }
    let key = HandleKey::from(slotmap::KeyData::from_ffi(handle));
    let mut guard = gc_arena().lock().unwrap_or_else(|e| e.into_inner());
    guard.0.mutate(|mc, root| match root.slots.get(key) {
        Some(&gc) => {
            let mut borrowed = gc.borrow_mut(mc);
            f(Some(&mut *borrowed))
        }
        None => f(None),
    })
}

/// Simultaneous immutable access to two entries. Handles may be equal
/// (single Gc lookup) or distinct. No lock-ordering complexity since
/// there is a single global arena lock.
///
/// **Do not call handle operations inside `f`** (same lock constraint).
pub fn with_two_entries<R>(
    ha: u64,
    hb: u64,
    f: impl FnOnce(Option<&Entry>, Option<&Entry>) -> R,
) -> R {
    let ka = HandleKey::from(slotmap::KeyData::from_ffi(ha));
    let kb = HandleKey::from(slotmap::KeyData::from_ffi(hb));
    let guard = gc_arena().lock().unwrap_or_else(|e| e.into_inner());
    guard.0.mutate(|_mc, root| {
        let ba = root.slots.get(ka).map(|&gc| gc.borrow());
        let bb = root.slots.get(kb).map(|&gc| gc.borrow());
        f(ba.as_deref(), bb.as_deref())
    })
}

/// Pays off incremental GC debt. Call at quiescence points (function
/// returns, scope exits) so the collector keeps pace with allocation.
pub fn collect_debt() {
    gc_arena()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .0
        .collect_debt();
}

/// Forces a full collection cycle. Used by the `gc.collect()` ABI.
pub fn finish_cycle() {
    gc_arena()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .0
        .finish_cycle();
}

/// Count of currently live handles (allocated minus freed).
pub fn live_handle_count() -> usize {
    LIVE_HANDLES.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_string_entry() {
        let h = alloc_entry(Entry::String(b"hello".to_vec()));
        assert_ne!(h, 0);
        with_entry(h, |entry| {
            assert!(matches!(entry, Some(Entry::String(b)) if b == b"hello"));
        });
        assert!(free_handle(h));
        with_entry(h, |entry| assert!(entry.is_none()));
    }

    #[test]
    fn stale_handle_rejected_after_free() {
        let h = alloc_entry(Entry::String(b"old".to_vec()));
        free_handle(h);
        // The slot is removed; same key should no longer resolve.
        with_entry(h, |entry| assert!(entry.is_none()));
    }

    #[test]
    fn invalid_handles_safe() {
        with_entry(0, |e| assert!(e.is_none()));
        with_entry(0xDEAD_BEEF_DEAD_BEEF, |e| assert!(e.is_none()));
        assert!(!free_handle(0));
    }

    #[test]
    fn double_free_returns_false() {
        let h = alloc_entry(Entry::String(b"x".to_vec()));
        assert!(free_handle(h));
        assert!(!free_handle(h));
    }

    #[test]
    fn with_entry_mut_modifies_in_place() {
        let h = alloc_entry(Entry::Vec(Box::new(vec![1, 2, 3])));
        with_entry_mut(h, |entry| {
            if let Some(Entry::Vec(v)) = entry {
                v.push(4);
            }
        });
        with_entry(h, |entry| {
            assert!(matches!(entry, Some(Entry::Vec(v)) if v.len() == 4));
        });
        free_handle(h);
    }

    #[test]
    fn with_two_entries_reads_both() {
        let ha = alloc_entry(Entry::String(b"foo".to_vec()));
        let hb = alloc_entry(Entry::String(b"bar".to_vec()));
        with_two_entries(ha, hb, |a, b| {
            assert!(matches!(a, Some(Entry::String(s)) if s == b"foo"));
            assert!(matches!(b, Some(Entry::String(s)) if s == b"bar"));
        });
        free_handle(ha);
        free_handle(hb);
    }

    #[test]
    fn live_count_tracks_alloc_free() {
        let before = live_handle_count();
        let h = alloc_entry(Entry::DateMs(0));
        assert_eq!(live_handle_count(), before + 1);
        free_handle(h);
        assert_eq!(live_handle_count(), before);
    }
}
