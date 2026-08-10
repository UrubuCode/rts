//! `rts ir` — dump the `rts-codegen-new` engine's per-function Cranelift IR to
//! stderr, without executing the program. Accepts a `.ts`/`.js` file path or an
//! inline source snippet (`rts ir "let x = 1 + 2; console.log(x)"`), mirroring
//! `eval`'s no-disk-imports behavior for snippets.
//!
//! NOT part of the `rts run`/`rts test` cutover: this stays a remaining
//! `rts-codegen-new` entry point. It has no equivalent on the `rts-host-rwk`
//! engine (no IR-dump path there yet).

use std::path::PathBuf;

use anyhow::{Result, anyhow};

use crate::compile_options::CompileOptions;

pub fn command(input: Option<String>, _options: CompileOptions) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts ir <input.ts | inline-source>"))?;
    let path = PathBuf::from(&input);
    let res = if path.exists() {
        rts_codegen_new::front::run::dump_ir_path(&path)
    } else if input.ends_with(".ts") || input.ends_with(".js") {
        return Err(anyhow!("input file not found: {}", path.display()));
    } else {
        // Not a file on disk and not named like one — treat as an inline snippet
        // (relative imports are not available in this form, like `eval`).
        rts_codegen_new::front::run::dump_ir_source(&input)
    };
    res.map_err(|e| anyhow!("{e}"))
}
