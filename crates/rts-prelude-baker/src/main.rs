//! `rts-prelude-baker` — step 10, slice 2.
//!
//! Compiles the engine's embedded stdlib prelude ONCE into a resident object
//! (`prelude.o`) + a metadata manifest (`prelude_manifest.bin`), so `rts run`
//! reuses the precompiled prelude instead of re-lowering + re-machine-compiling
//! it on every invocation (the fixed ~90 ms measured in step 10).
//!
//! Usage: `rts-prelude-baker <out-dir>`
//!   writes `<out-dir>/prelude.o` and `<out-dir>/prelude_manifest.bin`.
//!
//! Deterministic: the shape/gcell ids it bakes are assigned in source order, so
//! two bakes of the same prelude text produce byte-identical manifests (the
//! invariant the run path relies on — see `CRANELIFT_IMPLEMENTATION.md`).

use std::path::PathBuf;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let out_dir = PathBuf::from(
        args.next()
            .context("usage: rts-prelude-baker <out-dir>")?,
    );
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("create out dir {}", out_dir.display()))?;

    let baked = rts_codegen_new::front::run::bake::bake_prelude()
        .map_err(|e| anyhow::anyhow!("bake prelude: {e}"))?;

    let obj_bytes = baked.object_bytes();
    let obj_path = out_dir.join("prelude.o");
    std::fs::write(&obj_path, obj_bytes)
        .with_context(|| format!("write {}", obj_path.display()))?;

    let manifest_bytes = baked.manifest_bytes().map_err(|e| anyhow::anyhow!(e))?;
    let manifest_path = out_dir.join("prelude_manifest.bin");
    std::fs::write(&manifest_path, &manifest_bytes)
        .with_context(|| format!("write {}", manifest_path.display()))?;

    // The `@generated` address table compiled into `rts.exe` next to the linked-in
    // object, so the run path can register every resident prelude symbol on the JIT.
    let symbols_rs = baked.generated_symbol_table_rs();
    let symbols_path = out_dir.join("prelude_symbols.rs");
    std::fs::write(&symbols_path, symbols_rs.as_bytes())
        .with_context(|| format!("write {}", symbols_path.display()))?;

    let (fns, shapes, gcells) = baked.summary();
    eprintln!(
        "baked prelude: {fns} fns exported, {shapes} shapes, {gcells} gcells → {} ({} bytes obj, {} bytes manifest)",
        out_dir.display(),
        obj_bytes.len(),
        manifest_bytes.len(),
    );
    Ok(())
}
