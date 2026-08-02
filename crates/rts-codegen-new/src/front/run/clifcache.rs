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
//! ## WHERE IT PAYS, AND WHERE IT DOES NOT — measured, and the answer is narrow
//!
//! **It pays for repeated compiles of the SAME program.** `tiny.ts`, release:
//!
//! ```text
//! machine-compile, cold   15.6 ms    0 hits
//! machine-compile, warm   10.5 ms  203 hits     store 408 KB, stable
//! ```
//!
//! **It loses badly across a SUITE**, and no write policy fixes it. `rts test`
//! is 811 processes each compiling a different program:
//!
//! ```text
//! off  38.8 s     on  65.8 s   store 138 MB
//! off  37.7 s     on  78.2 s   store 171 MB
//! ```
//!
//! Two versions of the store were measured before that conclusion, because the
//! first failure had an obvious-looking cause that turned out to be only half of
//! it:
//!
//! 1. **Read-all + rewrite-all** (one file, `bincode` map): every process paid a
//!    full read AND a full rewrite of a growing file — quadratic in the number of
//!    programs. Suite 39.6 → 44.1 → 57.9 → 76.2 s.
//! 2. **Append-only** (this format), then **prelude-only** entries: the rewrite
//!    is gone and the writes are restricted to the functions that ought to
//!    repeat. It still loses, and the store still grows without converging.
//!
//! **The real reason is not the store at all: the prelude's stencils are
//! PROGRAM-DEPENDENT.** A prelude function's IR carries shape-id immediates
//! interned per program and IC-cell data symbols named from a per-run counter
//! (`ic.rs::CELL_CTR`, `aot_str.rs::DATA_CTR`), so the same source lowers to a
//! different stencil in each process and hashes to a different key. There is
//! nothing to hit. Making it converge means making those two things stable
//! across processes — which is a change to shape interning and data naming, not
//! to this file.
//!
//! So the honest scope is: **a repeated compile of one program**, where the
//! stencils genuinely repeat. That is a real case (an edit-run loop on a single
//! file), and it is why the code stays. It is not a default.
//!
//! ## The format
//!
//! Repeated `[32-byte key][u32 LE length][length bytes]`, append-only, capped at
//! [`MAX_BYTES`]. Append-only so a run never rewrites what other runs wrote;
//! length-prefixed so a torn tail (a killed process) is detectable — the reader
//! stops at the first truncated record instead of discarding the whole file.
//! Capped because an unbounded compile cache is how a `.rts/` directory silently
//! becomes gigabytes, which is exactly what the suite measurement above did.
//!
//! ## OPT-IN
//!
//! `RTS_CLIF_CACHE=1` (dir override `RTS_CLIF_CACHE_DIR`). `RTS_TIMING=1` reports
//! hits/misses so the trade is visible per run rather than assumed.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;
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

fn cache_path() -> PathBuf {
    if let Ok(dir) = std::env::var("RTS_CLIF_CACHE_DIR") {
        return PathBuf::from(dir).join("clifcache.bin");
    }
    PathBuf::from(".rts").join("clifcache.bin")
}

/// Parse the append-only log. Stops at the first truncated record rather than
/// discarding the file: a process killed mid-append leaves a partial tail, and
/// everything before it is still valid.
fn parse(bytes: &[u8]) -> HashMap<Key, Vec<u8>> {
    let mut out = HashMap::new();
    let mut p = 0usize;
    while p + 36 <= bytes.len() {
        let key: Key = match bytes[p..p + 32].try_into() {
            Ok(k) => k,
            Err(_) => break,
        };
        let len = u32::from_le_bytes([
            bytes[p + 32],
            bytes[p + 33],
            bytes[p + 34],
            bytes[p + 35],
        ]) as usize;
        let start = p + 36;
        let end = match start.checked_add(len) {
            Some(e) if e <= bytes.len() => e,
            _ => break, // truncated tail
        };
        out.insert(key, bytes[start..end].to_vec());
        p = end;
    }
    out
}

/// The process-wide loaded cache: the snapshot every worker reads, plus the
/// entries this run produced (appended on [`persist`]).
struct Cache {
    snapshot: Arc<HashMap<Key, Vec<u8>>>,
    added: HashMap<Key, Vec<u8>>,
    /// Size of the file as loaded — the budget check for new appends.
    on_disk: u64,
}

fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(|| {
        let bytes = std::fs::read(cache_path()).unwrap_or_default();
        let on_disk = bytes.len() as u64;
        Mutex::new(Cache {
            snapshot: Arc::new(parse(&bytes)),
            added: HashMap::new(),
            on_disk,
        })
    })
}

/// A per-worker view of the cache: shared immutable reads, buffered writes.
pub(super) struct WorkerStore {
    snapshot: Arc<HashMap<Key, Vec<u8>>>,
    new: Vec<(Key, Vec<u8>)>,
}

impl WorkerStore {
    fn new() -> Self {
        let snapshot = cache().lock().expect("clif cache poisoned").snapshot.clone();
        Self {
            snapshot,
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
        match self.snapshot.get(&k) {
            Some(v) => {
                HITS.fetch_add(1, Ordering::Relaxed);
                Some(Cow::Borrowed(v.as_slice()))
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

/// APPEND this run's new entries to the log. Never rewrites what is already
/// there, and stops appending once the file passes [`MAX_BYTES`].
pub(super) fn persist() {
    if !enabled() {
        return;
    }
    crate::timing::note("clif-cache hits", HITS.load(Ordering::Relaxed));
    crate::timing::note("clif-cache misses", MISSES.load(Ordering::Relaxed));

    let mut c = cache().lock().expect("clif cache poisoned");
    if c.added.is_empty() || c.on_disk >= MAX_BYTES {
        return;
    }
    // Only entries the loaded snapshot does not already have — a re-insert of a
    // key that hit would grow the log for nothing.
    let snapshot = c.snapshot.clone();
    let fresh: Vec<(Key, Vec<u8>)> = c
        .added
        .drain()
        .filter(|(k, _)| !snapshot.contains_key(k))
        .collect();
    if fresh.is_empty() {
        return;
    }

    let mut buf = Vec::new();
    for (k, v) in &fresh {
        buf.extend_from_slice(k);
        buf.extend_from_slice(&(v.len() as u32).to_le_bytes());
        buf.extend_from_slice(v);
    }

    let path = cache_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // A single `write_all` of one buffer in append mode is the closest this gets
    // to atomic across processes: `rts test` runs one process per test file and
    // they all append here. A torn tail is survivable by construction — `parse`
    // stops at the first truncated record.
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(&buf);
    }
    c.on_disk += buf.len() as u64;
}
