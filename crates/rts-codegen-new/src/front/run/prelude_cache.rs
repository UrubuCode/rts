//! Disk cache of the LOWERED PRELUDE (step 10, slice 1).
//!
//! The ~251 KB prelude `.ts` is identical every run, yet `rts run` re-parses and
//! re-lowers it (~47 ms) before compiling anything. This caches the lowered
//! result — the [`LoweredProgram`] plus the interned shape-registry snapshot it
//! produced as a side effect — keyed by a hash of the prelude text + a cache
//! version. On a hit, the run deserializes instead of lowering, and re-seeds the
//! shape registry so the cached `ClassDesc.global_shape` ids still resolve.
//!
//! This is behaviour-neutral: a miss (absent / stale / unreadable cache) falls
//! back to lowering exactly as before, and then writes the cache for next time.
//! The prelude machine code is still compiled every run in this slice — only the
//! parse+lower is skipped; resident prelude code is a later slice.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;

use rts_engine::heap::shapes;

use super::LoweredProgram;

/// Bump when the cache FORMAT or anything that changes the lowered output (the
/// engine's lowering, the HIR/ClassTable layout, the shape id scheme) changes, so
/// an old on-disk blob is never deserialized into an incompatible engine.
const CACHE_VERSION: u32 = 1;

/// The whole cached payload (owned, for DESERIALIZE): the lowered prelude + the
/// process-global state its lowering interned (shape id→keys, Error-class table).
/// gcell ids are NOT here — they are assigned post-merge every run
/// (`merge_programs`), not a side effect of the prelude lowering.
#[derive(serde::Deserialize)]
struct Cached {
    program: LoweredProgram,
    shapes: Vec<Vec<String>>,
    error_classes: Vec<(String, u32, Vec<String>)>,
}

/// The same payload borrowed, for SERIALIZE — avoids cloning the (large) lowered
/// program just to write it.
#[derive(serde::Serialize)]
struct CachedRef<'a> {
    program: &'a LoweredProgram,
    shapes: Vec<Vec<String>>,
    error_classes: Vec<(String, u32, Vec<String>)>,
}

/// The cache is ON by default; `RTS_NO_PRELUDE_CACHE=1` disables it (an escape
/// hatch for debugging / a corrupt cache dir).
///
/// It was briefly opt-in while a heap-corruption bug was open — the load path
/// seeded the shape registry unconditionally, which is illegal for a NESTED
/// compile (`new Function`/eval) that runs while the outer program's shapes are
/// live. That is fixed at the call site (`build_program_for_prelude` only uses
/// the cache when the shape registry is empty, i.e. the top-level compile), and
/// the full suite is now 723/8 = baseline under the cache — so it is safe on by
/// default.
fn enabled() -> bool {
    std::env::var_os("RTS_NO_PRELUDE_CACHE").is_none()
}

/// Content hash of the prelude text + cache version → the cache file stem.
/// `pub(super)` so the slice-2 baker keys its artifact by the same hash (the
/// run-path fallback triggers when a baked manifest's key ≠ the current prelude).
pub(super) fn key(prelude_src: &str) -> u64 {
    let mut h = DefaultHasher::new();
    CACHE_VERSION.hash(&mut h);
    prelude_src.hash(&mut h);
    h.finish()
}

fn cache_path(prelude_src: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("rts-prelude-cache");
    let _ = std::fs::create_dir_all(&p);
    p.push(format!("prelude_{:016x}.bin", key(prelude_src)));
    p
}

/// Try to load the cached lowered prelude. On success, RE-SEEDS the process shape
/// registry from the snapshot so the returned program's shape ids resolve, and
/// returns the program. `None` on any miss (absent / unreadable / decode error) —
/// the caller then lowers normally.
///
/// Must run on an EMPTY shape registry (the per-compile reset guarantees this);
/// [`shapes::seed_global_shapes`] asserts it.
pub(super) fn load(prelude_src: &str) -> Option<LoweredProgram> {
    if !enabled() {
        return None;
    }
    let bytes = std::fs::read(cache_path(prelude_src)).ok()?;
    let cached: Cached = bincode::deserialize(&bytes).ok()?;
    shapes::seed_global_shapes(cached.shapes);
    shapes::seed_error_classes(cached.error_classes);
    Some(cached.program)
}

/// Serialize the freshly-lowered prelude + the shape/Error snapshots it produced
/// to the cache file. Best-effort: any IO/encode failure is silently ignored (the
/// next run just lowers again). Written atomically (temp file + rename) so a
/// concurrent run never reads a half-written blob.
pub(super) fn store(prelude_src: &str, program: &LoweredProgram) {
    if !enabled() {
        return;
    }
    let cached = CachedRef {
        program,
        shapes: shapes::export_global_shapes(),
        error_classes: shapes::export_error_classes(),
    };
    let Ok(bytes) = bincode::serialize(&cached) else {
        return;
    };
    let final_path = cache_path(prelude_src);
    // Include the pid in the temp name so concurrent runs never fight over one
    // temp file; the rename is atomic and last-writer-wins (all writers produce
    // identical bytes for the same key, so the race is benign).
    let tmp = final_path.with_extension(format!("{}.tmp", std::process::id()));
    let ok = std::fs::File::create(&tmp)
        .and_then(|mut f| f.write_all(&bytes))
        .is_ok();
    if ok {
        let _ = std::fs::rename(&tmp, &final_path);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}
