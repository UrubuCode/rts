//! Whole-program JIT code cache (extends step 10 slice 2).
//!
//! `rts run file.ts` normally parses → lowers → machine-compiles the program every
//! time. With the resident prelude that is already ~8 user fns, but a LARGE user
//! program (or repeated runs of the same file) still re-does the whole pipeline.
//! This caches the COMPILED program — its machine bytes + symbolic relocs + the
//! lowered program + shape/gcell snapshot ([`super::bake::bake_program`]) — keyed
//! by the source text, and on a hit REPLAYS it ([`super::module_jit::compile_replay`])
//! instead of recompiling: only declare + `define_function_bytes` + finalize run.
//!
//! Opt-in via `RTS_JIT_CACHE=1` while it beds in; behaviour-neutral (a hit produces
//! byte-identical output to a compile — the manifest was produced BY a compile).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;

use crate::front::error::FrontResult;

use super::LoweredProgram;
use super::bake::PreludeManifest;
use super::module_jit::Program;

/// Bump when anything that changes the baked machine code (the engine's lowering,
/// the manifest format, the reloc scheme) changes, so a stale blob is never
/// replayed into an incompatible engine.
const CACHE_VERSION: u32 = 1;

/// Whether the whole-program JIT cache is enabled (opt-in during bring-up).
pub(super) fn enabled() -> bool {
    std::env::var_os("RTS_JIT_CACHE").is_some()
}

/// Cache key from the program SOURCE text + the prelude text (a prelude change
/// invalidates every program) + the cache version.
pub(super) fn key(program_src: &str) -> u64 {
    let mut h = DefaultHasher::new();
    CACHE_VERSION.hash(&mut h);
    program_src.hash(&mut h);
    super::registry::includes_prelude().hash(&mut h);
    h.finish()
}

fn cache_path(key: u64) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("rts-jit-cache");
    let _ = std::fs::create_dir_all(&p);
    p.push(format!("prog_{key:016x}.bin"));
    p
}

/// Load a cached program manifest for `key`, or `None` on any miss (absent /
/// unreadable / decode error — the caller then compiles normally).
fn load(key: u64) -> Option<PreludeManifest> {
    let bytes = std::fs::read(cache_path(key)).ok()?;
    bincode::deserialize(&bytes).ok()
}

/// Store `manifest` under `key` (best-effort, atomic temp+rename).
fn store(key: u64, manifest: &PreludeManifest) {
    let Ok(bytes) = bincode::serialize(manifest) else {
        return;
    };
    let final_path = cache_path(key);
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

/// Compile `program_src` to a runnable [`Program`], using the whole-program cache
/// when enabled: a HIT replays the cached machine code (no lower/compile); a MISS
/// builds via `build`, bakes the compiled program to the cache, and replays the
/// fresh manifest (uniform with the hit path).
///
/// When the cache is disabled, `build` + a normal compile is used (the caller's
/// `compile` closure), leaving today's behaviour untouched.
pub(super) fn compile_cached(
    program_src: &str,
    build: impl FnOnce() -> FrontResult<LoweredProgram>,
    compile: impl FnOnce(&LoweredProgram) -> FrontResult<Program>,
) -> FrontResult<Program> {
    if !enabled() {
        return compile(&build()?);
    }
    let k = key(program_src);
    if let Some(m) = load(k) {
        crate::timing::note("jit-cache: hit", 1);
        return super::module_jit::compile_replay(&m);
    }
    crate::timing::note("jit-cache: miss", 0);
    let prog = build()?;
    // Bake the WHOLE program (clear any resident-prelude marking so every fn —
    // prelude + user — is compiled into the manifest, self-contained for replay).
    let mut bakeable = prog.clone();
    bakeable.resident_import_names.clear();
    bakeable.resident_main = false;
    let manifest = super::bake::bake_program(&bakeable, k)?;
    store(k, &manifest);
    // Run from the fresh manifest (same replay path a later hit uses).
    super::module_jit::compile_replay(&manifest)
}
