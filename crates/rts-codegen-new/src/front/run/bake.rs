//! Step 10, slice 2 — BAKE the whole prelude to REPLAYABLE machine code.
//!
//! Slice 1 caches the LOWERED prelude (skips ~47 ms parse+lower) but still
//! machine-compiles it every run. Slice 2 captures the prelude's COMPILED machine
//! code once, ahead of time, and REPLAYS it into each run's JIT module via
//! `Module::define_function_bytes` — so the prelude lands in the run's OWN JIT
//! arena, right next to the user code, and the ~60 ms prelude machine-compile is
//! skipped.
//!
//! The correct-mechanism note (`CRANELIFT_IMPLEMENTATION.md` slice 2): this puts
//! the prelude in the SAME arena as the user code, so every call is NEAR — the
//! earlier "link `prelude.o` into `rts.exe`" delivery was wrong (it made the call
//! far, overflowing cranelift-jit's ±2 GB `X86CallPCRel4`). The capture reuses the
//! exact `{bytes, alignment, relocs}` triple `parcompile::compile_and_define`
//! already produces; relocs are stored SYMBOLICALLY (callee/data by name) so the
//! replay remaps them to each run's FuncIds. Baking the WHOLE prelude unpruned
//! keeps the fn set fixed, which is what makes the symbolic replay deterministic.

use cranelift_codegen::binemit::Reloc;
use cranelift_codegen::ir::{KnownSymbol, LibCall};
use cranelift_module::{DataId, FuncId, Module, ModuleDeclarations, ModuleReloc, ModuleRelocTarget};

use rts_engine::heap::shapes;

use crate::front::error::{FrontResult, Unsupported};

use super::LoweredProgram;

/// A relocation target, symbolized to a NAME (a run-specific FuncId/DataId is
/// meaningless across the bake→run boundary; the name is stable).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) enum SymTarget {
    /// A call/reference to another function, by its linkage name.
    Func(String),
    /// A reference to a data object (a baked string blob), by name.
    Data(String),
    /// A cranelift libcall (resolved by cranelift-jit itself at finalize).
    LibCall(LibCall),
    /// A linker-known symbol.
    Known(KnownSymbol),
    /// An offset within a named function (intra-function relocation).
    FuncOffset(String, u32),
}

/// One relocation of a baked function, with its target symbolized.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct SymReloc {
    pub offset: u32,
    pub kind: Reloc,
    pub addend: i64,
    pub target: SymTarget,
}

/// One baked (pre-compiled) prelude function: its machine bytes + symbolic relocs,
/// ready to `define_function_bytes` into a run's JIT module.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct BakedFn {
    pub name: String,
    pub alignment: u64,
    pub bytes: Vec<u8>,
    pub relocs: Vec<SymReloc>,
}

/// One baked string-data object (from AOT string mode), replayed via `define_data`.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct BakedData {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// The everything-serializable baked prelude: the metadata a user build needs (the
/// whole lowered prelude, so its classes/functions are ambient and the shape/error
/// ids resolve) + the replayable machine code (`funcs`/`data`) + the key that ties
/// it to the exact prelude text.
///
/// Crate-private: its `program` field is a `pub(crate)` [`LoweredProgram`], so the
/// type is not part of the crate's public surface.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct PreludeManifest {
    /// `prelude_cache::key(prelude_src)` — a hit requires this to match the
    /// current embedded prelude text (the fallback trigger when it does not).
    pub prelude_hash: u64,
    /// The WHOLE lowered prelude (unpruned) — ambient class table, function
    /// metadata, gcell ids, etc. the user build reads.
    pub program: LoweredProgram,
    /// The interned global-shape snapshot (`export_global_shapes`) — reseeded at
    /// run time so the ids baked into the prelude machine code resolve.
    pub shapes: Vec<Vec<String>>,
    /// The Error-class registry snapshot (`export_error_classes`).
    pub error_classes: Vec<(String, u32, Vec<String>)>,
    /// The generic class-shape registry snapshot (`export_class_shapes`) —
    /// name ↔ shape id for every prelude `class` (pickle revive / introspection).
    pub class_shapes: Vec<(String, u32)>,
    /// The number of prelude gcells (`0..gcell_count`).
    pub gcell_count: u32,
    /// The pre-compiled prelude functions (machine bytes + symbolic relocs).
    pub funcs: Vec<BakedFn>,
    /// The baked string-data objects the prelude machine code references.
    pub data: Vec<BakedData>,
}

/// The output of an ahead-of-time prelude bake. The manifest stays crate-private
/// (it embeds the crate-internal [`LoweredProgram`]); external drivers reach it
/// through the public accessors below.
pub struct BakedPrelude {
    manifest: PreludeManifest,
}

impl BakedPrelude {
    /// The manifest, bincode-serialized (write to `prelude_manifest.bin`).
    pub fn manifest_bytes(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(&self.manifest).map_err(|e| format!("serialize manifest: {e}"))
    }

    /// The manifest — crate-internal read access (tests).
    pub(crate) fn manifest(&self) -> &PreludeManifest {
        &self.manifest
    }

    /// Summary counters for the baker's log line — no internal type leaks.
    /// `(baked fns, data blobs, shapes, gcells)`.
    pub fn summary(&self) -> (usize, usize, usize, u32) {
        (
            self.manifest.funcs.len(),
            self.manifest.data.len(),
            self.manifest.shapes.len(),
            self.manifest.gcell_count,
        )
    }
}

/// Lower the embedded stdlib prelude, machine-compile it, and capture the compiled
/// bytes + symbolic relocs of every function (plus its string-data objects) for
/// replay. MUST run on a QUIESCENT process (empty shape registry) — it PRODUCES the
/// snapshot other runs seed.
pub fn bake_prelude() -> FrontResult<BakedPrelude> {
    crate::state::reset_codegen_state();
    assert_eq!(
        shapes::global_shape_count(),
        0,
        "bake_prelude must run on an empty shape registry"
    );

    let prelude_src = super::registry::includes_prelude();
    if prelude_src.is_empty() {
        return Err(Unsupported::new("no embedded prelude to bake").into());
    }
    let prelude_hash = super::prelude_cache::key(&prelude_src);

    let mut program = super::build_program(&prelude_src, super::PRELUDE_ARROW_NS)?;
    // Whole program IS the prelude → every fn is prelude-origin (privacy gate for
    // `engine.*`), matching what `merge_programs` sets in the normal flow.
    program.prelude_fns = program.funcs.iter().map(|f| f.name.clone()).collect();

    let gcell_count = program
        .gcells
        .values()
        .copied()
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);

    // Compile the prelude into a THROWAWAY JIT module, capturing every function's
    // machine bytes + relocs and every string blob. AOT string mode ON so string
    // literals become replayable DATA objects (a JIT-baked handle would be invalid
    // in the run process). Skip `__rts_startup` — the user build compiles the merged
    // main (prelude init prepended).
    let (funcs, data) = super::aot_str::with_aot_mode(|| capture_compiled(&program, true))?;

    let manifest = PreludeManifest {
        prelude_hash,
        program,
        shapes: shapes::export_global_shapes(),
        error_classes: shapes::export_error_classes(),
        class_shapes: shapes::export_class_shapes(),
        gcell_count,
        funcs,
        data,
    };
    Ok(BakedPrelude { manifest })
}

/// Compile `prog` into a scratch JIT module with capture on, returning the baked
/// functions (relocs symbolized) + the string blobs. `skip_main` drops
/// `__rts_startup` (the prelude bake — the user build compiles the merged main); the
/// whole-program cache keeps it as the entry.
fn capture_compiled(
    prog: &LoweredProgram,
    skip_main: bool,
) -> FrontResult<(Vec<BakedFn>, Vec<BakedData>)> {
    let mut module = super::module_jit::make_module();
    super::parcompile::begin_capture();
    super::aot_str::begin_data_capture();
    // Populate declares every fn/data + compiles + defines into the scratch module;
    // the capture hooks stash the bytes/relocs/blobs as a side effect.
    let populate = super::module_jit::populate_module(&mut module, prog);
    let emitted = super::parcompile::take_capture().unwrap_or_default();
    let blobs = super::aot_str::take_data_capture().unwrap_or_default();
    populate?;

    let decls = module.declarations();
    let mut funcs = Vec::with_capacity(emitted.len());
    for e in &emitted {
        // Use the DECLARATION name of the FuncId — `Emitted.name` carries the base
        // fn name even for a THUNK (whose real symbol is `<base>__rtsn_thunk`), and
        // the replay keys baked fns by their true symbol name.
        let name = decls.get_function_decl(e.id).linkage_name(e.id).into_owned();
        if skip_main && name == "__rts_startup" {
            continue;
        }
        let mut relocs = Vec::with_capacity(e.relocs.len());
        for r in &e.relocs {
            relocs.push(symbolize(r, decls)?);
        }
        funcs.push(BakedFn {
            name,
            alignment: e.alignment,
            bytes: e.bytes.clone(),
            relocs,
        });
    }
    let data = blobs
        .into_iter()
        .map(|(name, bytes)| BakedData { name, bytes })
        .collect();
    Ok((funcs, data))
}

/// Bake a WHOLE already-lowered program (prelude + user, INCLUDING `__rts_startup`)
/// to a replayable manifest — the whole-program JIT code cache. `prelude_hash`
/// carries the CACHE KEY (the program's own text/version hash), computed by the
/// caller. MUST run on a quiescent shape registry state matching the program's
/// (the caller resets + builds `prog`, leaving its shapes interned).
pub(crate) fn bake_program(prog: &LoweredProgram, cache_key: u64) -> FrontResult<PreludeManifest> {
    let gcell_count = prog.gcells.values().copied().max().map(|m| m + 1).unwrap_or(0);
    // Keep `__rts_startup` — it is the entry the cache replays and runs.
    let (funcs, data) = super::aot_str::with_aot_mode(|| capture_compiled(prog, false))?;
    Ok(PreludeManifest {
        prelude_hash: cache_key,
        program: prog.clone(),
        shapes: shapes::export_global_shapes(),
        error_classes: shapes::export_error_classes(),
        class_shapes: shapes::export_class_shapes(),
        gcell_count,
        funcs,
        data,
    })
}

/// Convert a run-specific [`ModuleReloc`] into a name-symbolic [`SymReloc`] using
/// the scratch module's declarations (FuncId/DataId → linkage name).
fn symbolize(r: &ModuleReloc, decls: &ModuleDeclarations) -> FrontResult<SymReloc> {
    let target = match &r.name {
        ModuleRelocTarget::User {
            namespace: 0,
            index,
        } => {
            let id = FuncId::from_u32(*index);
            SymTarget::Func(decls.get_function_decl(id).linkage_name(id).into_owned())
        }
        ModuleRelocTarget::User {
            namespace: 1,
            index,
        } => {
            let id = DataId::from_u32(*index);
            SymTarget::Data(decls.get_data_decl(id).linkage_name(id).into_owned())
        }
        ModuleRelocTarget::User { namespace, index } => {
            return Err(Unsupported::new(format!(
                "unexpected reloc namespace {namespace} (index {index})"
            ))
            .into());
        }
        ModuleRelocTarget::LibCall(lc) => SymTarget::LibCall(*lc),
        ModuleRelocTarget::KnownSymbol(ks) => SymTarget::Known(*ks),
        ModuleRelocTarget::FunctionOffset(fid, off) => {
            SymTarget::FuncOffset(decls.get_function_decl(*fid).linkage_name(*fid).into_owned(), *off)
        }
    };
    Ok(SymReloc {
        offset: r.offset,
        kind: r.kind,
        addend: r.addend,
        target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The baked machine code + symbolic relocs must be produced deterministically
    /// (same fn count, same symbolic reloc targets) across bakes of the same
    /// prelude text — the run seeds shape/gcell ids by position and replays these
    /// bytes, so any drift is a silent miscompile.
    #[test]
    #[ignore = "drains process-global engine state; run serially/explicitly"]
    fn bake_is_deterministic() {
        let a = bake_prelude().expect("first bake");
        let b = bake_prelude().expect("second bake");
        let (ma, mb) = (a.manifest(), b.manifest());
        assert_eq!(ma.prelude_hash, mb.prelude_hash, "prelude hash");
        assert_eq!(ma.shapes, mb.shapes, "shape snapshot");
        assert_eq!(ma.gcell_count, mb.gcell_count, "gcell count");
        assert_eq!(ma.funcs.len(), mb.funcs.len(), "baked fn count");
        assert_eq!(ma.data.len(), mb.data.len(), "baked data count");
        // The per-fn symbolic reloc TARGETS must match (bytes may vary if the
        // baker's data-symbol counter shifts, but the symbolic structure is stable).
        for (fa, fb) in ma.funcs.iter().zip(&mb.funcs) {
            assert_eq!(fa.name, fb.name, "baked fn order/name");
            assert_eq!(fa.relocs.len(), fb.relocs.len(), "reloc count for {}", fa.name);
        }
    }

    /// END-TO-END: bake the prelude, install the manifest, enable the resident
    /// path, and RENDER a prelude-heavy program — the output must match a normal
    /// (fallback) render. This exercises the whole `define_function_bytes` replay
    /// in-process (no binary build): if the baked prelude machine code replays and
    /// runs correctly, the resident path is sound.
    #[test]
    #[ignore = "drains process-global engine state + sets RTS_RESIDENT; run serially"]
    fn resident_replay_roundtrip() {
        const SRC: &str = "console.log(\"hi \" + (2+3));\n\
             const a = [1,2,3,4].map(x => x*2).filter(x => x>4);\n\
             console.log(a.join(\",\"));\n\
             console.log(\"upper \" + \"abc\".toUpperCase());\n\
             class E extends Error { constructor(m){ super(m); this.name=\"E\"; } }\n\
             try { throw new E(\"boom\"); } catch(e){ console.log(e.name + \":\" + e.message); }\n\
             const m = new Map(); m.set(\"k\", 42); console.log(\"map \" + m.get(\"k\"));\n\
             const o = { x: 1, y: 2 }; console.log(\"keys \" + Object.keys(o).join(\",\"));\n\
             console.log([3,1,2].sort().join(\"-\"));\n\
             console.log(JSON.stringify({a:1,b:[2,3]}));";

        // Baseline (fallback) render.
        crate::state::reset_codegen_state();
        let expected = super::super::render_source(SRC).expect("baseline render");

        // Bake + install → the resident path engages on `is_installed` (no env
        // flag; `RTS_NO_RESIDENT` would DISABLE it). Render again.
        let baked = bake_prelude().expect("bake");
        super::super::resident::install(baked.manifest_bytes().expect("manifest bytes"));
        crate::state::reset_codegen_state();
        let got = super::super::render_source(SRC).expect("resident render");

        assert_eq!(got, expected, "resident replay output must equal fallback");
    }

    /// WHOLE-PROGRAM CACHE proof: bake an ENTIRE program (prelude + user + main) to
    /// a manifest, then run it purely by replaying the baked machine code
    /// (`compile_replay`) — no parse/lower/compile — and assert the output matches a
    /// normal render. Validates that a compiled program can be cached and replayed.
    #[test]
    #[ignore = "drains process-global engine state; run serially"]
    fn whole_program_cache_roundtrip() {
        const SRC: &str = "function fib(n){ return n<2 ? n : fib(n-1)+fib(n-2); }\n\
             console.log(\"fib \" + fib(10));\n\
             const a = [5,3,8,1].sort((x,y)=>x-y);\n\
             console.log(a.join(\",\"));\n\
             class P { constructor(x){ this.x=x; } dbl(){ return this.x*2; } }\n\
             console.log(\"p \" + new P(21).dbl());\n\
             console.log(JSON.stringify({k:[1,2,3]}));";

        // Baseline render (compiles + runs).
        crate::state::reset_codegen_state();
        let expected = super::super::render_source(SRC).expect("baseline render");

        // Bake the WHOLE compiled program (no resident prelude installed here, so the
        // merged program compiles every fn — prelude + user — into the manifest).
        crate::state::reset_codegen_state();
        let prog = super::super::build_with_includes(SRC).expect("build");
        let manifest = bake_program(&prog, 0xC0FFEE).expect("bake program");

        // Replay: run PURELY from the baked bytes (no lower, no compile).
        crate::state::reset_codegen_state();
        let program =
            super::super::module_jit::compile_replay(&manifest).expect("compile_replay");
        let ((), out) =
            crate::value::abi_adapter::with_capture(|| program.run_main());

        assert_eq!(out, expected, "whole-program replay output must equal a normal run");
    }

    /// DISK CACHE end-to-end: with `RTS_JIT_CACHE=1`, render a program TWICE — the
    /// first run is a MISS (compiles + stores), the second is a HIT (replays from
    /// disk). Both must equal a normal (uncached) render.
    #[test]
    #[ignore = "drains process-global engine state + sets RTS_JIT_CACHE + touches temp; run serially"]
    fn jit_cache_miss_then_hit() {
        // Unique source so the temp-cache key is fresh (no cross-run pollution).
        let src = "console.log(\"cache \" + (7*6));\nconsole.log([9,2,5].sort().join(\"|\"));\n\
                   // marker jit_cache_miss_then_hit v1\n";

        crate::state::reset_codegen_state();
        let expected = super::super::render_source(src).expect("baseline");

        // SAFETY: single-threaded ignored test; restored below.
        unsafe { std::env::set_var("RTS_JIT_CACHE", "1") };
        crate::state::reset_codegen_state();
        let miss = super::super::render_source(src).expect("miss render");
        crate::state::reset_codegen_state();
        let hit = super::super::render_source(src).expect("hit render");
        unsafe { std::env::remove_var("RTS_JIT_CACHE") };

        assert_eq!(miss, expected, "cache MISS output must equal a normal run");
        assert_eq!(hit, expected, "cache HIT (replayed) output must equal a normal run");
    }

    /// AOT-from-manifest: bake a program, then emit an AOT object by REPLAYING the
    /// baked machine code into an `ObjectModule` (no per-fn compile). Asserts the
    /// object emits successfully and is substantial (a real `.o`, not a stub) — the
    /// full proof (link + run) is a binary-level test.
    #[cfg(not(target_os = "macos"))]
    #[test]
    #[ignore = "drains process-global engine state; run serially"]
    fn aot_replay_emits_object() {
        const SRC: &str = "console.log(\"aot \" + (6*7));\n\
             function sq(n){ return n*n; }\n\
             console.log(sq(9));";
        crate::state::reset_codegen_state();
        let prog = super::super::build_with_includes(SRC).expect("build");
        let manifest = bake_program(&prog, 0xA07).expect("bake program");

        crate::state::reset_codegen_state();
        let obj = super::super::module_aot::compile_replay_aot(&manifest)
            .expect("compile_replay_aot");
        assert!(
            obj.len() > 1024,
            "AOT replay object suspiciously small ({} bytes)",
            obj.len()
        );
    }
}
