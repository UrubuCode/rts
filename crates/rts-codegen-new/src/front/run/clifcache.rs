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
//! ## Backing policy (ours, not Cranelift's)
//!
//! The trait is a plain `[u8] -> Vec<u8>` map, so the policy is the embedder's.
//! One file per key would cost ~425 file opens per run on Windows, which is the
//! thing being optimized — so this stores **one file** (`.rts/clifcache.bin`),
//! read once into a `HashMap` at first use and written back once at the end of
//! the run when new entries were produced.
//!
//! The parallel machine-compile phase cannot share a `&mut dyn CacheKvStore`, so
//! each worker gets a [`WorkerStore`]: reads go to a shared immutable snapshot,
//! writes accumulate in a per-worker `Vec` that [`merge_worker`] folds back in
//! after the parallel region.
//!
//! ## OPT-IN, by measurement
//!
//! `RTS_CLIF_CACHE=1` enables it. It is off by default because the cost it adds
//! (SHA-256 over every function's IR + a `postcard` round trip) is paid on every
//! function whether or not it hits, and on a program whose functions are mostly
//! tiny that can exceed the compilation it avoids. `RTS_TIMING=1` reports the
//! hit count so the trade is visible per run rather than assumed.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cranelift_codegen::incremental_cache::CacheKvStore;

type Key = [u8; 32];

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

/// The process-wide loaded cache: the snapshot every worker reads, plus the
/// entries this run produced (written back on [`persist`]).
struct Cache {
    snapshot: Arc<HashMap<Key, Vec<u8>>>,
    added: HashMap<Key, Vec<u8>>,
}

fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(|| {
        let loaded: HashMap<Key, Vec<u8>> = std::fs::read(cache_path())
            .ok()
            .and_then(|bytes| bincode::deserialize(&bytes).ok())
            .unwrap_or_default();
        Mutex::new(Cache {
            snapshot: Arc::new(loaded),
            added: HashMap::new(),
        })
    })
}

/// A per-worker view of the cache: shared immutable reads, buffered writes.
pub(super) struct WorkerStore {
    snapshot: Arc<HashMap<Key, Vec<u8>>>,
    new: Vec<(Key, Vec<u8>)>,
}

impl WorkerStore {
    pub(super) fn new() -> Self {
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

/// Write the cache back when this run added entries. One file, one write.
pub(super) fn persist() {
    if !enabled() {
        return;
    }
    let (hits, misses) = (
        HITS.load(Ordering::Relaxed),
        MISSES.load(Ordering::Relaxed),
    );
    crate::timing::note("clif-cache hits", hits);
    crate::timing::note("clif-cache misses", misses);

    let mut c = cache().lock().expect("clif cache poisoned");
    if c.added.is_empty() {
        return;
    }
    let mut merged: HashMap<Key, Vec<u8>> = (*c.snapshot).clone();
    merged.extend(c.added.drain());
    let Ok(bytes) = bincode::serialize(&merged) else {
        return;
    };
    let path = cache_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Write to a temp sibling then rename, so a concurrent reader never sees a
    // half-written file (`rts test` runs one process per test file).
    let tmp = path.with_extension(format!("bin.tmp{}", std::process::id()));
    if std::fs::write(&tmp, &bytes).is_ok() && std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    c.snapshot = Arc::new(merged);
}
