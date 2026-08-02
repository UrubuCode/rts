//! CRANELIFT INCREMENTAL COMPILATION CACHE — a `CacheKvStore` over `.rts/`.
//!
//! `cranelift-codegen` ships a per-FUNCTION incremental cache behind its
//! `incremental-cache` feature: `Context::compile_with_cache` hashes the
//! function's **stencil** (SHA-256 over the IR plus the target triple and the
//! compiler/ISA flags) and, on a hit, deserializes the previously emitted
//! machine code instead of running regalloc + the egraph + emission.
//!
//! ## Why the granularity is the interesting part
//!
//! Cranelift splits the IR into a *stencil* and *parameters*, and compilation
//! depends only on the stencil. **Function-reference relocations and debug
//! source locations are parameters, not stencil** — so a function whose callees
//! were assigned different `FuncId`s in this run still hits. That is exactly the
//! remap `bake.rs::symbolize` does by hand for the whole-program bake, except
//! upstream and per function: editing one source file invalidates only the
//! functions whose IR actually changed.
//!
//! ## WHERE IT PAYS — measured, in four rounds
//!
//! **It pays for repeated compiles of the SAME program**, which is the edit-run
//! loop. `tiny.ts`, release:
//!
//! ```text
//! machine-compile, cold   15.6 ms    0 hits
//! machine-compile, warm   10.4 ms  203 hits     store 408 KB, stable
//! ```
//!
//! **Across a SUITE it is roughly break-even and no longer diverges.** `rts test`
//! is 811 processes each compiling a different program:
//!
//! ```text
//! off  37.8 s     on  42.5 s then 40.1 s     store 34.6 MB, CONVERGED
//! ```
//!
//! Getting there took four rounds, and each one was a different mistake:
//!
//! 1. **Read-all + rewrite-all** (one file, `bincode` map): every process paid a
//!    full read AND a full rewrite of a growing file — quadratic in the number of
//!    programs. Suite 39.6 → 44.1 → 57.9 → 76.2 s, store 69 MB.
//! 2. **Append-only + prelude-only entries**: the rewrite is gone and writes are
//!    restricted to the functions that ought to repeat. Still 65.8 → 78.2 s, and
//!    the store STILL grew without converging — which is what proved the store
//!    was not the problem.
//! 3. **Deterministic IC-cell names** (`ic.rs`): the actual fix, below.
//! 4. **Index + blob split, with a size-chosen access mode**: the format, below.
//!
//! Which located the actual cause, and it was NOT the store: **the prelude's
//! stencils were PROGRAM-DEPENDENT.** A prelude function's IR carried IC-cell
//! data symbols named from a process-wide counter, and prelude PRUNING keeps a
//! different subset per program — so every downstream cell number shifted and the
//! same source hashed to a different key in every process. Nothing could hit.
//!
//! `ic.rs` now names cells `__rtsic_<fn>_<site>`, per owning function, which
//! removes the coupling. Measured immediately after that change, using one
//! program's cache to compile a DIFFERENT program:
//!
//! ```text
//! tiny.ts      cold     0 hits / 207 misses
//! numbench.ts           203 hits /   4 misses      <- different program
//! callbench.ts          116 hits /  93 misses      <- different program, more code
//! ```
//!
//! Cross-program hits went from impossible to 98%, and the store converges
//! instead of growing forever. The remaining suite gap is I/O volume, which is
//! what the format below is for.
//!
//! ## The format: a small INDEX plus a blob file, with the access mode chosen by size
//!
//! Two append-only files. `clifcache.idx` is `[32-byte key][u64 offset][u32 len]`
//! records; `clifcache.bin` is the concatenated blobs. A process always reads the
//! INDEX whole (44 bytes per entry) and then gets the blobs one of two ways —
//! because the two workloads want opposite things, and trying to serve both with
//! one policy is what rounds 1 and 2 above were:
//!
//! * store ≤ [`INLINE_MAX`] — slurp the blob file once, serve hits from memory.
//!   The single-program case lives here (~400 KB, ~200 hits per run), and serving
//!   those same hits by individual `seek_read` measured SLOWER: 14.6-16.7 ms of
//!   machine-compile against 10.4 ms slurped, i.e. 200 syscalls cost more than
//!   one sequential 400 KB read.
//! * store > [`INLINE_MAX`] — read only the blobs that hit, by offset, via
//!   `seek_read`/`read_at`. Those take `&self`, so a worker serves `get(&self)`
//!   with no lock and no shared file position. The suite case lives here, where
//!   slurping tens of MB in each of 811 processes is precisely the I/O that made
//!   earlier rounds lose to no cache at all.
//!
//! Append-only so a run never rewrites what other runs wrote; data written before
//! its index record so an offset never points past the end of the blob file;
//! capped at [`MAX_BYTES`] because an unbounded compile cache is how a `.rts/`
//! directory silently becomes gigabytes, which is what round 1 did.
//!
//! ## OPT-IN
//!
//! `RTS_CLIF_CACHE=1` (dir override `RTS_CLIF_CACHE_DIR`). `RTS_TIMING=1` reports
//! hits/misses so the trade is visible per run rather than assumed.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Seek, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cranelift_codegen::incremental_cache::CacheKvStore;

type Key = [u8; 32];

/// Ceiling on the on-disk store. Past this, reads still hit and writes are
/// dropped. 32 MB is ~80x the size a single program's prelude entries occupy
/// (408 KB measured), so the case this cache is actually for has enormous
/// headroom, while the case it is NOT for — a suite of hundreds of distinct
/// programs, which drove an uncapped store to 171 MB — stops early instead of
/// eating the disk.
const MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Below this size the whole data file is slurped once per process and served
/// from memory; above it, blobs are read individually by offset. See `cache()`
/// for the measurement that sets the two modes against each other.
const INLINE_MAX: u64 = 4 * 1024 * 1024;

/// Hits and misses of the current process, reported under `RTS_TIMING=1`.
static HITS: AtomicUsize = AtomicUsize::new(0);
static MISSES: AtomicUsize = AtomicUsize::new(0);

/// Is the cache enabled for this process? `RTS_CLIF_CACHE=1`.
pub(super) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_CLIF_CACHE")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    })
}

fn cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("RTS_CLIF_CACHE_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(".rts")
}

fn data_path() -> PathBuf {
    cache_dir().join("clifcache.bin")
}

fn index_path() -> PathBuf {
    cache_dir().join("clifcache.idx")
}

/// One index record: 32-byte key + u64 LE data offset + u32 LE length.
const IDX_REC: usize = 44;

/// Parse the append-only INDEX. Stops at the first truncated record rather than
/// discarding the file: a process killed mid-append leaves a partial tail, and
/// everything before it is still valid.
fn parse_index(bytes: &[u8]) -> HashMap<Key, (u64, u32)> {
    let mut out = HashMap::new();
    let mut p = 0usize;
    while p + IDX_REC <= bytes.len() {
        let Ok(key) = <Key>::try_from(&bytes[p..p + 32]) else {
            break;
        };
        let off = u64::from_le_bytes(bytes[p + 32..p + 40].try_into().expect("8 bytes"));
        let len = u32::from_le_bytes(bytes[p + 40..p + 44].try_into().expect("4 bytes"));
        out.insert(key, (off, len));
        p += IDX_REC;
    }
    out
}

/// Read `len` bytes at `off` without moving a shared cursor. `seek_read`/`read_at`
/// take `&self`, which is what lets a worker serve `get(&self)` straight from
/// disk with no lock and no shared file position.
fn read_at(f: &std::fs::File, off: u64, len: u32) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; len as usize];
    let mut done = 0usize;
    while done < buf.len() {
        #[cfg(windows)]
        let n = {
            use std::os::windows::fs::FileExt;
            f.seek_read(&mut buf[done..], off + done as u64).ok()?
        };
        #[cfg(unix)]
        let n = {
            use std::os::unix::fs::FileExt;
            f.read_at(&mut buf[done..], off + done as u64).ok()?
        };
        if n == 0 {
            return None;
        }
        done += n;
    }
    Some(buf)
}

/// The process-wide loaded cache: the INDEX (small, read once) plus the entries
/// this run produced (appended by [`persist`]).
struct Cache {
    index: Arc<HashMap<Key, (u64, u32)>>,
    /// The whole data file, when it is small enough to be worth slurping — see
    /// [`INLINE_MAX`]. `None` means workers read blobs by offset instead.
    inline: Option<Arc<Vec<u8>>>,
    added: HashMap<Key, Vec<u8>>,
    /// Size of the data file as loaded — the budget check for new appends.
    data_len: u64,
}

fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(|| {
        let idx = std::fs::read(index_path()).unwrap_or_default();
        let data_len = std::fs::metadata(data_path()).map(|m| m.len()).unwrap_or(0);
        // Two access modes, chosen by SIZE, because the two workloads want
        // opposite things and the measurement says so:
        //
        // * ONE program compiled repeatedly — the store is ~400 KB and a run hits
        //   ~200 entries. Slurping it costs one sequential read; serving those
        //   hits by `seek_read` instead cost 200 syscalls and measured SLOWER
        //   (14.6-16.7 ms of machine-compile against 10.5 ms slurped).
        // * A suite of hundreds of distinct programs — the store reaches tens of
        //   MB, and slurping it in every one of 811 processes is the I/O that made
        //   the cache lose to no cache at all.
        let inline = (data_len <= INLINE_MAX)
            .then(|| std::fs::read(data_path()).ok().map(Arc::new))
            .flatten();
        Mutex::new(Cache {
            index: Arc::new(parse_index(&idx)),
            inline,
            added: HashMap::new(),
            data_len,
        })
    })
}

/// A per-worker view: the shared index, its own read handle on the data file,
/// and a buffer of the entries this worker compiled.
pub(super) struct WorkerStore {
    index: Arc<HashMap<Key, (u64, u32)>>,
    /// The slurped data file, when the store is small enough for it.
    inline: Option<Arc<Vec<u8>>>,
    /// Read handle used only when `inline` is `None`.
    data: Option<std::fs::File>,
    new: Vec<(Key, Vec<u8>)>,
}

impl WorkerStore {
    fn new() -> Self {
        let (index, inline) = {
            let c = cache().lock().expect("clif cache poisoned");
            (c.index.clone(), c.inline.clone())
        };
        let data = inline.is_none().then(|| std::fs::File::open(data_path()).ok()).flatten();
        Self {
            index,
            inline,
            data,
            new: Vec::new(),
        }
    }

    /// One store per WORKER, or `None` when the cache is off — the shape
    /// `rayon::map_init` wants.
    pub(super) fn init() -> Option<Self> {
        enabled().then(Self::new)
    }
}

impl Drop for WorkerStore {
    /// Fold this worker's newly compiled entries back into the process cache.
    /// On `Drop` rather than an explicit `merge` because `rayon::map_init` never
    /// hands the per-worker init value back to the caller.
    fn drop(&mut self) {
        if self.new.is_empty() {
            return;
        }
        let taken = std::mem::take(&mut self.new);
        if let Ok(mut c) = cache().lock() {
            c.added.extend(taken);
        }
    }
}

impl CacheKvStore for WorkerStore {
    fn get(&self, key: &[u8]) -> Option<Cow<'_, [u8]>> {
        let k: Key = key.try_into().ok()?;
        let Some(&(off, len)) = self.index.get(&k) else {
            MISSES.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        // Slurped mode: the blob is already in memory, so this is a slice.
        if let Some(all) = self.inline.as_ref() {
            let (start, end) = (off as usize, off as usize + len as usize);
            if end <= all.len() {
                HITS.fetch_add(1, Ordering::Relaxed);
                return Some(Cow::Owned(all[start..end].to_vec()));
            }
            MISSES.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        // The index says the entry exists; a failed read means a truncated or
        // concurrently-rotated data file, which is a MISS, never an error — the
        // caller then simply compiles the function.
        match self.data.as_ref().and_then(|f| read_at(f, off, len)) {
            Some(v) => {
                HITS.fetch_add(1, Ordering::Relaxed);
                Some(Cow::Owned(v))
            }
            None => {
                MISSES.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    fn insert(&mut self, key: &[u8], val: Vec<u8>) {
        if let Ok(k) = <Key>::try_from(key) {
            self.new.push((k, val));
        }
    }
}

/// APPEND this run's new entries: blobs to the data file, `(key, offset, len)`
/// records to the index. Never rewrites what is already there, and stops once
/// the data file passes [`MAX_BYTES`].
pub(super) fn persist() {
    if !enabled() {
        return;
    }
    crate::timing::note("clif-cache hits", HITS.load(Ordering::Relaxed));
    crate::timing::note("clif-cache misses", MISSES.load(Ordering::Relaxed));

    let mut c = cache().lock().expect("clif cache poisoned");
    if c.added.is_empty() || c.data_len >= MAX_BYTES {
        return;
    }
    // Only entries the loaded index does not already have — re-appending a key
    // that hit would grow both files for nothing.
    let index = c.index.clone();
    let fresh: Vec<(Key, Vec<u8>)> = c
        .added
        .drain()
        .filter(|(k, _)| !index.contains_key(k))
        .collect();
    if fresh.is_empty() {
        return;
    }

    let _ = std::fs::create_dir_all(cache_dir());
    let Ok(mut data) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(data_path())
    else {
        return;
    };
    // The base offset is read from the FILE, not from the loaded `data_len`:
    // `rts test` runs one process per test file and they all append here, so
    // another process may have grown it since we loaded. Each run then writes
    // index records describing where its own bytes actually landed.
    let Ok(base) = data.seek(std::io::SeekFrom::End(0)) else {
        return;
    };

    let mut blob = Vec::new();
    let mut idx = Vec::with_capacity(fresh.len() * IDX_REC);
    for (k, v) in &fresh {
        idx.extend_from_slice(k);
        idx.extend_from_slice(&(base + blob.len() as u64).to_le_bytes());
        idx.extend_from_slice(&(v.len() as u32).to_le_bytes());
        blob.extend_from_slice(v);
    }

    // DATA FIRST, then the index. An index record is a promise that the bytes are
    // already on disk; writing it first would let another process read an offset
    // into a region that does not exist yet. Crashing between the two leaks blob
    // bytes, which is harmless — no index record points at them.
    if data.write_all(&blob).is_err() {
        return;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(index_path())
    {
        let _ = f.write_all(&idx);
    }
    c.data_len = base + blob.len() as u64;
}
