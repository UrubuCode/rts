//! Step 10, slice 2 — BAKE the whole prelude to a RESIDENT object (`prelude.o`)
//! + a metadata manifest.
//!
//! Slice 1 caches the LOWERED prelude (skips ~47 ms parse+lower) but still
//! machine-compiles it every run. Slice 2 goes the rest of the way: compile the
//! WHOLE prelude ONCE, ahead of time, into a relocatable object whose functions
//! are `Linkage::Export`, link it into `rts.exe`, and at run time DECLARE those
//! functions `Import` in the user module + register their resident addresses on
//! the `JITBuilder` — so a `rts run` never re-compiles the prelude at all.
//!
//! This module owns the AHEAD-OF-TIME half: [`bake_prelude`] lowers the prelude
//! (unpruned — the whole thing is resident, so per-run pruning is dropped) and
//! emits the object bytes + a [`PreludeManifest`]. The `rts-prelude-baker`
//! workspace bin drives it at build time; the run-path consumer is wired in a
//! later commit (behind the behaviour-neutral fallback in `build_with_includes`).
//!
//! The verdict from the feasibility spike (`CRANELIFT_IMPLEMENTATION.md`) is path
//! B — real-linker name resolution, NOT a byte cache — because FuncId reloc
//! indices are not deterministic across programs. So the baked artifact is a
//! plain object linked normally; nothing here replays raw relocations.

use std::cell::Cell;

use cranelift_module::{Linkage, Module};

use rts_engine::heap::shapes;

use crate::front::error::{FrontResult, Unsupported};

use super::LoweredProgram;

thread_local! {
    /// While set, every USER function / thunk / new-thunk / `main` declared by
    /// [`super::module_jit::populate_module`] is `Linkage::Export` instead of the
    /// default `Local` — so a separately-linked user module can `Import` them.
    /// Off for the ordinary JIT (`Local`) and AOT (`Local`, self-contained) paths.
    static BAKE_EXPORT: Cell<bool> = const { Cell::new(false) };
}

/// The exported symbol name of the baked prelude's top-level init (`__rtsn_main`
/// renamed so it never collides with the USER program's own `__rtsn_main`). The
/// run path calls this first to run the prelude's module-level initializers
/// (`const console = new Console()`, `rts:test` hook globals).
pub const PRELUDE_MAIN_SYMBOL: &str = "__rtsn_prelude_main";

/// The linkage a user function / thunk / `main` gets at declare time: `Export`
/// while baking (the prelude object publishes them), `Local` otherwise.
///
/// LOAD-BEARING cranelift assumption: several body-building sites re-declare an
/// already-declared prelude function as `Linkage::Local` (call/dispatch/ctor/TCO
/// emit), and cranelift-module's `Linkage::merge(Export, Local) == Export` keeps
/// the export. A cranelift bump that changed that merge rule would silently
/// downgrade prelude exports to Local (the resident symbols would vanish); the
/// determinism test checks the manifest, not the object symbol table, so it would
/// not catch it. Pin this if bumping cranelift.
pub(super) fn user_linkage() -> Linkage {
    if BAKE_EXPORT.with(Cell::get) {
        Linkage::Export
    } else {
        Linkage::Local
    }
}

/// True while [`bake_prelude`] is compiling the resident prelude object.
pub(super) fn is_baking() -> bool {
    BAKE_EXPORT.with(Cell::get)
}

/// The everything-serializable half of a baked prelude: the metadata a user
/// build needs (the whole lowered prelude, so its classes/functions are ambient
/// and the shape/error ids resolve) plus the resident partition (exported symbol
/// names, gcell count) and the key that ties it to the exact prelude text.
///
/// Crate-private: its `program` field is a `pub(crate)` [`LoweredProgram`], so the
/// type is not part of the crate's public surface. The baker consumes it only as
/// opaque serialized bytes + the summary counters exposed on [`BakedPrelude`].
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct PreludeManifest {
    /// `prelude_cache::key(prelude_src)` — a hit requires this to match the
    /// current embedded prelude text (the fallback trigger when it does not).
    pub prelude_hash: u64,
    /// The WHOLE lowered prelude (unpruned). Carries the ambient class table,
    /// function metadata, gcell ids, captures, etc. the user build reads — the
    /// same payload `prelude_cache` serializes, minus the per-run prune.
    pub program: LoweredProgram,
    /// The interned global-shape snapshot (`export_global_shapes`) — reseeded at
    /// run time so the ids baked as immediates in the prelude object resolve.
    pub shapes: Vec<Vec<String>>,
    /// The Error-class registry snapshot (`export_error_classes`).
    pub error_classes: Vec<(String, u32, Vec<String>)>,
    /// Every symbol the prelude object EXPORTS (user fns + their thunks + class
    /// new-thunks + `PRELUDE_MAIN_SYMBOL`). The run path declares each `Import`
    /// and registers its resident address on the `JITBuilder`.
    pub export_symbols: Vec<String>,
    /// The number of prelude gcells (`0..gcell_count`). User gcells must be
    /// offset by this base so a baked prelude gcell id never collides with a
    /// freshly-numbered user one.
    pub gcell_count: u32,
}

/// The output of an ahead-of-time prelude bake: the relocatable object bytes
/// (COFF/ELF/Mach-O per host) + the manifest. The manifest stays crate-private
/// (it embeds the crate-internal [`LoweredProgram`]); external drivers (the
/// `rts-prelude-baker` bin) reach it through the public accessors below.
pub struct BakedPrelude {
    object: Vec<u8>,
    manifest: PreludeManifest,
}

impl BakedPrelude {
    /// The relocatable prelude object bytes (write to `prelude.o`).
    pub fn object_bytes(&self) -> &[u8] {
        &self.object
    }

    /// The manifest, bincode-serialized (write to `prelude_manifest.bin`). Errors
    /// only on an encoding failure (never expected for this fixed shape).
    pub fn manifest_bytes(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(&self.manifest).map_err(|e| format!("serialize manifest: {e}"))
    }

    /// The manifest — crate-internal read access (tests, run-path consumer).
    pub(crate) fn manifest(&self) -> &PreludeManifest {
        &self.manifest
    }

    /// Summary counters for the baker's log line — no internal type leaks.
    pub fn summary(&self) -> (usize, usize, u32) {
        (
            self.manifest.export_symbols.len(),
            self.manifest.shapes.len(),
            self.manifest.gcell_count,
        )
    }
}

/// Lower the embedded stdlib prelude and machine-compile it to a resident object
/// with every prelude function `Export`ed, returning the object bytes + manifest.
///
/// MUST run on a QUIESCENT process (empty shape registry) — it seeds nothing, it
/// PRODUCES the snapshot other runs seed. Mirrors [`super::module_aot`] (same ISA
/// flags, `aot_str` ON so no process-local string handle is baked) but declares
/// the prelude functions `Export` and emits NO CRT `main` entry.
pub fn bake_prelude() -> FrontResult<BakedPrelude> {
    // A fresh, empty codegen state: the shape ids we snapshot must start at the
    // base, exactly as a top-level run interns them.
    rts_adapters::state::reset_codegen_state();
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

    // Lower the WHOLE prelude (no user side, no merge, no prune — it is all
    // resident). Same arrow namespace the real prelude build uses so the baked
    // arrow symbol names match what the user build expects to import.
    let mut program = super::build_program(&prelude_src, super::PRELUDE_ARROW_NS)?;
    // The WHOLE program IS the prelude, so EVERY function is prelude-origin and may
    // use the PRIVATE `engine.*` API (`engine.trace_capture()` in `Error`'s ctor).
    // `build_program` leaves `prelude_fns` empty (its safe user-only default);
    // `merge_programs` sets it to the prelude's own fn names in the normal flow —
    // do the same here so the privacy gate admits the prelude's own functions.
    program.prelude_fns = program.funcs.iter().map(|f| f.name.clone()).collect();

    // The resident partition: gcell ids are exactly what this prelude-only build
    // numbered (dense from 0), so their count is the user offset base.
    let gcell_count = program
        .gcells
        .values()
        .copied()
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);

    // Every symbol the object will EXPORT — reconstructed from the program the
    // same way `populate_module` declares them (fns, one thunk each, a new-thunk
    // per class with a real ctor, plus the renamed prelude main).
    let export_symbols = collect_export_symbols(&program);

    // Emit the object with Export linkage + AOT string mode.
    let object = super::aot_str::with_aot_mode(|| {
        with_bake_export(|| emit_prelude_object(&program))
    })?;

    let manifest = PreludeManifest {
        prelude_hash,
        program,
        shapes: shapes::export_global_shapes(),
        error_classes: shapes::export_error_classes(),
        export_symbols,
        gcell_count,
    };
    Ok(BakedPrelude { object, manifest })
}

/// The full set of symbols [`emit_prelude_object`] publishes, derived from the
/// same rules `populate_module` uses to declare them.
fn collect_export_symbols(prog: &LoweredProgram) -> Vec<String> {
    let mut syms: Vec<String> = Vec::new();
    for f in &prog.funcs {
        syms.push(f.name.clone());
        syms.push(super::thunk::thunk_name(&f.name));
    }
    for desc in prog.classes.iter() {
        // A real synthesized ctor (not a `__rtsl_noctor_*` literal placeholder)
        // gets a new-thunk — the same gate `populate_module` applies.
        if prog.funcs.iter().any(|f| f.name == desc.ctor) {
            syms.push(super::thunk::new_thunk_name(&desc.name));
        }
    }
    syms.push(PRELUDE_MAIN_SYMBOL.to_string());
    syms.sort();
    syms.dedup();
    syms
}

/// Lower `prog` into a fresh `ObjectModule` with Export linkage (via the
/// [`BAKE_EXPORT`] flag `populate_module` consults) and return the object bytes.
/// No CRT `main` entry — the prelude is a library of resident functions.
fn emit_prelude_object(prog: &LoweredProgram) -> FrontResult<Vec<u8>> {
    let mut module = super::module_aot::make_object_module()?;
    super::module_jit::populate_module(&mut module, prog)?;
    let product = module.finish();
    product
        .emit()
        .map_err(|e| Unsupported::new(format!("emit prelude object: {e}")).into())
}

/// Run `f` with the bake-export linkage flag set, restoring it after — via a
/// `Drop` guard so a PANIC in `f` still restores it. Without the guard a panic
/// mid-bake would leave `BAKE_EXPORT=true`, silently turning every later ordinary
/// JIT compile in the SAME process into an export-everything build (the run-path
/// consumer calls `bake_prelude` in the `rts` process, so this matters).
fn with_bake_export<T>(f: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            BAKE_EXPORT.with(|c| c.set(self.0));
        }
    }
    let _g = Restore(BAKE_EXPORT.with(|c| c.replace(true)));
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The IDS baked as immediates in the prelude object (shape ids, gcell ids)
    /// MUST be identical across bakes of the same prelude text — otherwise a
    /// resident prelude compiled by one bake and seeded from another's manifest
    /// would read shape/gcell ids that no longer line up (a silent miscompile /
    /// SIGILL, the highest-severity risk in the slice plan). The on-disk manifest
    /// bytes may differ (HashMap serialization order), but the id-bearing data —
    /// the ordered shape snapshot, the gcell name→id map, the export symbol set,
    /// the gcell count, the prelude hash — must match exactly.
    ///
    /// `#[ignore]` because [`bake_prelude`] does a FULL `reset_codegen_state`
    /// (drains the process-global shape registry) — hostile to the parallel lib
    /// suite, whose `with_engine` guard deliberately keeps shapes across tests.
    /// Run explicitly: `cargo test -p rts-codegen-new --lib bake_ -- --ignored`.
    #[test]
    #[ignore = "drains process-global engine state; run serially/explicitly"]
    fn bake_is_deterministic_on_ids() {
        let a = bake_prelude().expect("first bake");
        let b = bake_prelude().expect("second bake");
        let (ma, mb) = (a.manifest(), b.manifest());
        assert_eq!(ma.prelude_hash, mb.prelude_hash, "prelude hash");
        assert_eq!(ma.shapes, mb.shapes, "ordered shape snapshot (shape ids)");
        assert_eq!(ma.gcell_count, mb.gcell_count, "gcell count (user offset base)");
        assert_eq!(
            ma.program.gcells, mb.program.gcells,
            "gcell name→id map (baked immediates)"
        );
        assert_eq!(ma.export_symbols, mb.export_symbols, "exported symbol set");
        assert_eq!(ma.error_classes, mb.error_classes, "error-class snapshot");
    }
}
