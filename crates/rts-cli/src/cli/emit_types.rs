//! `rts emit-types` — generate `rts.d.ts` from the live `rts-codegen-new`
//! Registry (classes + `rts:`/`node:` modules + globals), using each
//! `Member.ts_signature` plus the `///` doc comments the `#[rtse::class]`
//! macro captures.
//!
//! NOT part of the `rts run`/`rts test` cutover: stays a remaining
//! `rts-codegen-new` entry point — the `rts-host-rwk` engine has no `.d.ts`
//! export path yet.

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
