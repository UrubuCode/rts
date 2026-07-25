//! `rts emit-types` — generate `rts.d.ts` from the live engine Registry (classes
//! + `rts:`/`node:` modules + globals), using each `Member.ts_signature` plus the
//! `///` doc comments the `#[rtse::class]` macro captures. Revived on the new
//! engine (was stubbed at the P5 cutover; the catalog now lives in the Registry,
//! iterated deterministically by `rts_codegen_new::emit_dts`).

use anyhow::{Context, Result};

pub fn command(output: Option<String>) -> Result<()> {
    let dts = rts_codegen_new::emit_dts();
    match output {
        Some(path) => {
            std::fs::write(&path, &dts).with_context(|| format!("writing {path}"))?;
            println!("wrote {path} ({} bytes)", dts.len());
        }
        None => print!("{dts}"),
    }
    Ok(())
}
