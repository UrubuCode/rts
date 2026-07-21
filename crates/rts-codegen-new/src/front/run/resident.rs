//! Step 10, slice 2 — the RUN-PATH side of the resident prelude.
//!
//! The `rts-prelude-baker` produces a manifest holding the prelude's PRE-COMPILED
//! machine code (`BakedFn` bytes + symbolic relocs) + string blobs. The root bin
//! installs the manifest bytes here at startup ([`install`]). When the consumer
//! engages (`front::run::mark_resident_imports`), the prelude functions are
//! declared but NOT compiled from IR; instead [`replay`] defines them into the
//! run's JIT module via `define_function_bytes`, so the prelude code lands in the
//! run's OWN arena (near the user code — every call is a near call) and the ~60 ms
//! prelude machine-compile is skipped.
//!
//! INERT until installed: a binary built without a baked prelude never calls
//! [`install`], so `is_installed()` is false and the run path keeps the fallback.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use cranelift_module::{
    DataDescription, DataId, FuncId, Linkage, Module, ModuleReloc, ModuleRelocTarget,
};

use super::bake::{PreludeManifest, SymTarget};

fn slot() -> &'static Mutex<Option<Vec<u8>>> {
    static R: OnceLock<Mutex<Option<Vec<u8>>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(None))
}

/// Install the baked prelude manifest (bincode bytes) produced by the baker and
/// embedded in this binary. Called once by the root bin at startup. A binary built
/// without a baked prelude never calls this.
pub fn install(manifest_bytes: Vec<u8>) {
    *slot().lock().expect("resident slot poisoned") = Some(manifest_bytes);
}

/// Whether a baked prelude manifest is installed — the gate the run path checks.
pub(crate) fn is_installed() -> bool {
    slot().lock().map(|g| g.is_some()).unwrap_or(false)
}

/// Deserialize the installed manifest, or `None` when none is installed / it fails
/// to decode (a corrupt artifact falls back to the non-resident path).
pub(crate) fn manifest() -> Option<PreludeManifest> {
    let g = slot().lock().ok()?;
    let bytes = g.as_ref()?;
    bincode::deserialize(bytes).ok()
}

/// Define every RESIDENT prelude function (and its string-data objects) into
/// `module` from the baked machine code, using `ids` (the name→FuncId map
/// `populate_module` built when it declared the prelude fns `Local`). Only the
/// functions in `resident_names` are replayed; the rest of `manifest.funcs` are
/// declared-but-not-referenced (harmless).
///
/// Reloc targets are remapped from the baked NAMES to this run's ids: a prelude fn
/// resolves via `ids`; a runtime extern is declared `Import` (its address comes
/// from the `JITBuilder::symbol` registration) and resolved by name; a data blob
/// resolves to the `DataId` defined here.
pub(crate) fn replay(
    module: &mut dyn Module,
    declared: &HashMap<String, FuncId>,
    to_define: &std::collections::HashSet<String>,
) -> Result<(), String> {
    let manifest = manifest().ok_or("resident manifest missing at replay")?;

    // 1. Define every baked string-data object; record name→DataId.
    let mut data_ids: HashMap<String, DataId> = HashMap::new();
    for d in &manifest.data {
        let id = module
            .declare_data(&d.name, Linkage::Local, false, false)
            .map_err(|e| format!("declare data `{}`: {e}", d.name))?;
        let mut desc = DataDescription::new();
        desc.define(d.bytes.clone().into_boxed_slice());
        module
            .define_data(id, &desc)
            .map_err(|e| format!("define data `{}`: {e}", d.name))?;
        data_ids.insert(d.name.clone(), id);
    }

    // 2. Resolve a reloc-target NAME to a run FuncId: a prelude fn (in `ids`), an
    //    already-declared symbol, or a freshly-declared Import (address via the
    //    JIT symbol table). Cached so each extern is declared once.
    let mut fn_ids: HashMap<String, FuncId> = declared.clone();
    let mut resolve_fn = |module: &mut dyn Module, name: &str| -> Result<FuncId, String> {
        if let Some(id) = fn_ids.get(name) {
            return Ok(*id);
        }
        // Already declared elsewhere (a user call to the same runtime extern)?
        if let Some(cranelift_module::FuncOrDataId::Func(id)) =
            module.declarations().get_name(name)
        {
            fn_ids.insert(name.to_string(), id);
            return Ok(id);
        }
        // A runtime extern only referenced by the prelude: declare Import. The
        // signature is irrelevant for a reloc-only reference (no call IR is
        // emitted; the address is resolved by name from the JIT symbol table).
        let sig = module.make_signature();
        let id = module
            .declare_function(name, Linkage::Import, &sig)
            .map_err(|e| format!("declare extern `{name}`: {e}"))?;
        fn_ids.insert(name.to_string(), id);
        Ok(id)
    };

    // 3. Define each resident function/thunk from its baked bytes + remapped relocs.
    let baked_names: std::collections::HashSet<&String> =
        manifest.funcs.iter().map(|f| &f.name).collect();
    let missing: Vec<&String> = to_define
        .iter()
        .filter(|n| !baked_names.contains(n))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "{} to_define names have no baked bytes, e.g. {:?}",
            missing.len(),
            &missing[..missing.len().min(5)]
        ));
    }
    for f in &manifest.funcs {
        if !to_define.contains(&f.name) {
            continue;
        }
        let Some(&fid) = declared.get(&f.name) else {
            return Err(format!("resident fn `{}` was not declared", f.name));
        };
        let mut relocs: Vec<ModuleReloc> = Vec::with_capacity(f.relocs.len());
        for r in &f.relocs {
            let name = ModuleRelocTarget::from(match &r.target {
                SymTarget::Func(n) => resolve_fn(module, n)?,
                SymTarget::FuncOffset(n, off) => {
                    let id = resolve_fn(module, n)?;
                    relocs.push(ModuleReloc {
                        offset: r.offset,
                        kind: r.kind,
                        name: ModuleRelocTarget::FunctionOffset(id, *off),
                        addend: r.addend,
                    });
                    continue;
                }
                SymTarget::Data(n) => {
                    let id = *data_ids
                        .get(n)
                        .ok_or_else(|| format!("resident data `{n}` missing"))?;
                    relocs.push(ModuleReloc {
                        offset: r.offset,
                        kind: r.kind,
                        name: ModuleRelocTarget::from(id),
                        addend: r.addend,
                    });
                    continue;
                }
                SymTarget::LibCall(lc) => {
                    relocs.push(ModuleReloc {
                        offset: r.offset,
                        kind: r.kind,
                        name: ModuleRelocTarget::LibCall(*lc),
                        addend: r.addend,
                    });
                    continue;
                }
                SymTarget::Known(ks) => {
                    relocs.push(ModuleReloc {
                        offset: r.offset,
                        kind: r.kind,
                        name: ModuleRelocTarget::KnownSymbol(*ks),
                        addend: r.addend,
                    });
                    continue;
                }
            });
            relocs.push(ModuleReloc {
                offset: r.offset,
                kind: r.kind,
                name,
                addend: r.addend,
            });
        }
        module
            .define_function_bytes(fid, f.alignment, &f.bytes, &relocs)
            .map_err(|e| format!("define resident fn `{}`: {e}", f.name))?;
    }
    Ok(())
}
