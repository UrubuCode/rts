//! Whole-program compile cache — the JIT manifest and the AOT object.
//!
//! `rts run file.ts` normally parses → lowers → machine-compiles the program every
//! time, and `rts compile file.ts` does the same again for the object. This caches
//! the COMPILED program — its machine bytes + symbolic relocs + the lowered
//! program + shape/gcell snapshot ([`super::bake::bake_program`]) — and, for AOT,
//! the emitted object bytes. On a hit `run` REPLAYS the manifest
//! ([`super::module_jit::compile_replay`]) instead of recompiling, and `compile`
//! returns the cached object verbatim.
//!
//! Layout and naming are [`super::cachedir`]'s: one slot per SOURCE PATH, three
//! artefacts (`.bin` manifest, `.obj` object, `.meta` validity header), under
//! `node_modules/.rts/` in a project and `%TEMP%/.rts/` otherwise.
//!
//! **Opt-in** (`RTS_JIT_CACHE=1`) — see [`enabled`] for the measurements that say
//! why, and for the one open defect.
//!
//! ## Only a program that HAS a path is cached
//!
//! [`super::run_source`] / [`super::render_source`] compile a STRING and are not
//! cached, by construction — `entry: Option<&Path>` is `None` for them and every
//! entry point below misses. Two reasons, and the second is the hard one:
//!
//! 1. A string has no stable identity to key a slot on, so it could only ever be
//!    content-named — one file per distinct source, forever.
//! 2. **Replay mutates process-global state.** `compile_replay` calls
//!    `reset_global_shapes` + `seed_global_shapes` so the shape ids baked into the
//!    machine code resolve. That is correct for a CLI process running ONE program
//!    and wrong for a process running many: caching the string paths made the
//!    in-process unit suite go from 849 passed / 6 failed to 109 passed / 746
//!    failed, ending in `global shape registry poisoned` and a non-unwinding abort,
//!    because parallel tests reset each other's registry. A per-program reset is
//!    only sound when the program IS the process.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::front::error::FrontResult;

use super::LoweredProgram;
use super::bake::PreludeManifest;
use super::cachedir::{self, Slot};
use super::module_jit::Program;

/// Bump when anything that changes the baked machine code (the engine's lowering,
/// the manifest format, the reloc scheme) changes, so a stale blob is never
/// replayed into an incompatible engine.
///
/// v2: the class-identity fix in `rts_engine::heap::shapes`. This cache replays
/// BAKED MACHINE CODE whose shape-id immediates were produced by the engine that
/// stored it, and `rts compile` reuses it too, so without this bump a machine that
/// ran the pre-fix binary once would keep replaying the vulnerable dispatch —
/// silently, for both `run` and `compile`.
///
/// v4: path-keyed slots + the `.meta` sidecar (the key moved out of the filename).
/// v5: Tier 2.2 int overflow checks ON by default. Same hazard as v2 and one step
/// worse: this cache replays BAKED MACHINE CODE keyed on the program text, so a
/// machine that ran the pre-change binary once would keep replaying WRAPPING
/// arithmetic — a wrong answer, not a stale layout — for both `run` and `compile`.
const CACHE_VERSION: u32 = 5;

/// Whether the compile cache is enabled. **OPT-IN** via `RTS_JIT_CACHE=1`.
///
/// It is opt-in on MEASUREMENT, not caution. For ONE program it is a clear win:
/// `rts run` 100 ms → 73 ms, `rts compile` 1046 ms → 956 ms (the AOT floor is
/// `rust-lld`, not codegen). Two results say it must not be the default yet:
///
/// - **It is a net LOSS on a batch.** The full TS suite is 38 s uncached and 82 s
///   with a WARM cache (92 s cold). `rts test` spawns one child per file, so ~16
///   processes each read their own ~1.1 MB manifest; that I/O costs more than
///   compiling these small programs.
/// - **It costs 1017 MB for 805 files.** Every entry bakes its own copy of the
///   whole prelude, so the prelude is stored 805 times.
///
/// Both have ONE cause — a whole-program manifest — and one fix: cache per MODULE
/// so the prelude is a single shared slot. Until that lands, `rts run`'s win does
/// not pay for `rts test`'s loss.
///
/// There is also an open defect: `tests/node_url.test.ts` HANGS under replay (it
/// needs `new URL` + `searchParams` + `keys()`/`values()` together; no smaller
/// repro found). Shipping that on by default would be a hang committed as a pass.
pub(super) fn enabled() -> bool {
    matches!(
        std::env::var("RTS_JIT_CACHE").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// Identity of the engine binary itself, folded into every cache key.
///
/// `CACHE_VERSION` is a hand-maintained constant: anyone who changes the
/// lowering and forgets to bump it makes every machine replay machine code
/// built by a different engine — silently, and for `rts compile` too. That is a
/// tolerable footgun for an opt-in flag and not one for a default. Mixing in the
/// executable's own length and mtime means a rebuilt engine cannot read an older
/// engine's blobs, whether or not anyone remembered to bump the constant.
fn build_identity() -> u64 {
    let mut h = DefaultHasher::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(md) = std::fs::metadata(&exe) {
            md.len().hash(&mut h);
            if let Ok(t) = md.modified() {
                if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                    d.as_nanos().hash(&mut h);
                }
            }
        }
        exe.hash(&mut h);
    }
    h.finish()
}

/// Content key of a program: its entry SOURCE text + the prelude text (a prelude
/// change invalidates every program) + the cache version + the engine build
/// identity.
///
/// The entry text alone is NOT sufficient — a program's imports are not in it.
/// That hole is closed by [`Meta::inputs`], because the import set is not knowable
/// before parsing and a hit must not parse.
fn key(program_src: &str) -> u64 {
    let mut h = DefaultHasher::new();
    CACHE_VERSION.hash(&mut h);
    build_identity().hash(&mut h);
    program_src.hash(&mut h);
    super::registry::includes_prelude().hash(&mut h);
    h.finish()
}

/// The validity header stored beside the artefacts, read before either of them.
///
/// It exists because the slot is named by PATH, so the filename no longer proves
/// anything about the contents (it used to be the content key). Reading this
/// first also means a stale entry costs one small `bincode` decode instead of
/// deserializing a multi-megabyte manifest that is about to be thrown away.
#[derive(serde::Serialize, serde::Deserialize)]
struct Meta {
    /// [`key`] of the program that produced the artefacts.
    key: u64,
    /// The canonical entry path. Compared on load so a 64-bit path-hash
    /// COLLISION is detected and treated as a miss rather than replaying an
    /// unrelated program.
    entry: PathBuf,
    /// Every source file the resolver read, with a hash of its contents.
    inputs: Vec<PathBuf>,
    /// Content hash per entry of [`Self::inputs`], same order.
    input_hashes: Vec<u64>,
}

impl Meta {
    /// `true` when this header still describes the program at `entry` with
    /// content key `want_key` — same program, same engine, and every dependency
    /// unchanged on disk.
    fn is_valid_for(&self, entry: &Path, want_key: u64) -> bool {
        if self.key != want_key {
            return false;
        }
        let canon = std::fs::canonicalize(entry).unwrap_or_else(|_| entry.to_path_buf());
        if self.entry != canon {
            return false;
        }
        if self.inputs.len() != self.input_hashes.len() {
            return false;
        }
        let pairs: Vec<_> = self
            .inputs
            .iter()
            .cloned()
            .zip(self.input_hashes.iter().copied())
            .collect();
        crate::front::modules::graph::inputs_unchanged(&pairs)
    }
}

/// Read + validate the header of `slot`, or `None` on any miss (absent /
/// unreadable / decode error / **a dependency changed**).
///
/// The dependency check is what makes a multi-file program correct under the
/// cache: the key covers only the entry text, so editing an imported module
/// leaves it identical. Re-hashing the recorded inputs costs one read per module
/// (microseconds) against the ~28 ms a hit saves.
fn valid_meta(slot: &Slot, entry: &Path, want_key: u64) -> bool {
    let Ok(bytes) = std::fs::read(slot.meta()) else {
        return false;
    };
    let Ok(meta) = bincode::deserialize::<Meta>(&bytes) else {
        return false;
    };
    if !meta.is_valid_for(entry, want_key) {
        crate::timing::note("compile-cache: stale", 1);
        return false;
    }
    true
}

/// Write `bytes` to `path`, atomically (temp + rename), best-effort.
///
/// The temp name carries the pid so two `rts` processes compiling the same file
/// cannot write each other's partial file; the rename then makes whichever
/// finishes last the winner, which is fine because both wrote the same content.
fn store_file(path: &Path, bytes: &[u8]) {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let ok = std::fs::File::create(&tmp)
        .and_then(|mut f| f.write_all(bytes))
        .is_ok();
    if ok {
        let _ = std::fs::rename(&tmp, path);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Write the validity header for `slot` — always LAST, after the artefacts it
/// describes, so an interrupted write leaves a slot with no valid header (a
/// miss) rather than a header promising an artefact that is not there.
///
/// `inputs` is the drained dependency ledger of the build that produced those
/// artefacts (see [`take_inputs`]).
fn store_meta(slot: &Slot, entry: &Path, key: u64, inputs: &[(PathBuf, u64)]) {
    let (inputs, input_hashes): (Vec<_>, Vec<_>) = inputs.iter().cloned().unzip();
    let meta = Meta {
        key,
        entry: std::fs::canonicalize(entry).unwrap_or_else(|_| entry.to_path_buf()),
        inputs,
        input_hashes,
    };
    if let Ok(bytes) = bincode::serialize(&meta) {
        store_file(&slot.meta(), &bytes);
    }
}

/// Drain the resolver's dependency ledger — every file the build just read, with
/// a content hash. Must be called right after `build`, before anything else can
/// load a graph and overwrite it.
fn take_inputs() -> Vec<(PathBuf, u64)> {
    crate::front::modules::graph::take_inputs()
}

/// Load the manifest for the program at `entry`, or `None` on any miss.
fn load_manifest(slot: &Slot, entry: &Path, want_key: u64) -> Option<PreludeManifest> {
    if !valid_meta(slot, entry, want_key) {
        return None;
    }
    let bytes = std::fs::read(slot.bin()).ok()?;
    bincode::deserialize(&bytes).ok()
}

/// Bake `prog` into a manifest, store it (+ its header) in `slot`, and return it.
///
/// The whole program is baked — any resident-prelude marking is cleared first so
/// every function (prelude + user) is in the manifest and the blob is
/// self-contained for replay.
fn bake_and_store(
    slot: &Slot,
    entry: &Path,
    key: u64,
    prog: &LoweredProgram,
    inputs: &[(PathBuf, u64)],
) -> FrontResult<PreludeManifest> {
    let mut bakeable = prog.clone();
    bakeable.resident_import_names.clear();
    bakeable.resident_main = false;
    let manifest = super::bake::bake_program(&bakeable, key)?;
    slot.ensure_dir();
    if let Ok(bytes) = bincode::serialize(&manifest) {
        store_file(&slot.bin(), &bytes);
        store_meta(slot, entry, key, inputs);
    }
    Ok(manifest)
}

/// Compile the program at `entry` to a runnable [`Program`]: a HIT replays the
/// cached machine code (no lower, no compile); a MISS builds via `build`, bakes
/// the compiled program into the cache, and replays the fresh manifest (uniform
/// with the hit path).
///
/// `entry` is `None` for a string program, which is never cached — see the module
/// header. When the cache is disabled, `build` + the caller's `compile` runs, so
/// the uncached behaviour is exactly today's.
pub(super) fn compile_cached(
    entry: Option<&Path>,
    program_src: &str,
    build: impl FnOnce() -> FrontResult<LoweredProgram>,
    compile: impl FnOnce(&LoweredProgram) -> FrontResult<Program>,
) -> FrontResult<Program> {
    let Some(entry) = entry.filter(|_| enabled()) else {
        return compile(&build()?);
    };
    let k = key(program_src);
    let slot = cachedir::slot(Some(entry), k);
    if let Some(m) = load_manifest(&slot, entry, k) {
        crate::timing::note("compile-cache: hit", 1);
        return super::module_jit::compile_replay(&m);
    }
    crate::timing::note("compile-cache: miss", 0);
    // Start the dependency ledger clean. `ModuleGraph::load` resets it too, but a
    // build that never loads a graph would otherwise inherit the previous
    // program's input list and then miss spuriously whenever those unrelated
    // files changed.
    crate::front::modules::graph::reset_inputs();
    let prog = build()?;
    let inputs = take_inputs();
    let manifest = bake_and_store(&slot, entry, k, &prog, &inputs)?;
    // Run from the fresh manifest (same replay path a later hit uses).
    super::module_jit::compile_replay(&manifest)
}

/// Emit the AOT object for the program at `entry`, using the cache.
///
/// Three tiers, cheapest first:
///
/// 1. a valid `.obj` — the object bytes verbatim, nothing compiled at all;
/// 2. a valid `.bin` — replay the baked machine code into a fresh
///    `ObjectModule` (skips parse/lower/compile), then store the `.obj`;
/// 3. a miss — `build` + `emit`, then store the `.obj` so the next compile lands
///    in tier 1.
///
/// Tier 3 STORING is the point: before this, the AOT path only ever READ the
/// cache, so a project that never ran `rts run` recompiled from scratch every
/// single time — `rts compile` twice in a row was two full compiles.
///
/// It deliberately does not bake a `.bin` here. Baking runs a scratch compile of
/// every function to capture its bytes, so it would roughly double the cost of
/// the miss to populate an artefact only `rts run` reads. A later `rts run` bakes
/// it into the same slot under the same key, and the two artefacts coexist.
///
/// **Not on macOS**: its AOT codegen sets `is_pic` and the JIT's does not, so the
/// baked bytes are not interchangeable there. macOS always takes tier 3 and
/// stores nothing.
pub(super) fn compile_object_cached(
    entry: &Path,
    build: impl FnOnce() -> FrontResult<LoweredProgram>,
    emit: impl FnOnce(&LoweredProgram) -> FrontResult<Vec<u8>>,
    replay: impl FnOnce(&PreludeManifest) -> FrontResult<Vec<u8>>,
) -> FrontResult<Vec<u8>> {
    if cfg!(target_os = "macos") || !enabled() {
        return emit(&build()?);
    }
    let k = key(&std::fs::read_to_string(entry).unwrap_or_default());
    let slot = cachedir::slot(Some(entry), k);

    if valid_meta(&slot, entry, k) {
        if let Ok(bytes) = std::fs::read(slot.obj()) {
            crate::timing::note("compile-cache: aot object hit", 1);
            return Ok(bytes);
        }
        if let Ok(bytes) = std::fs::read(slot.bin()) {
            if let Ok(m) = bincode::deserialize::<PreludeManifest>(&bytes) {
                crate::timing::note("compile-cache: aot replay into object", 1);
                let obj = replay(&m)?;
                slot.ensure_dir();
                store_file(&slot.obj(), &obj);
                return Ok(obj);
            }
        }
    }

    crate::timing::note("compile-cache: aot miss", 0);
    crate::front::modules::graph::reset_inputs();
    let prog = build()?;
    let inputs = take_inputs();
    let obj = emit(&prog)?;
    slot.ensure_dir();
    store_file(&slot.obj(), &obj);
    // Header LAST — it is what makes the object above readable next time.
    store_meta(&slot, entry, k, &inputs);
    Ok(obj)
}
