//! `rts ir` — dump the NEW engine's IR for a program, without running it.
//!
//! Accepts a `.ts`/`.js` file path or an inline source snippet
//! (`rts ir "let x = 1 + 2; console.log(x)"`), mirroring `eval`'s
//! no-disk-imports behavior for snippets.
//!
//! # What changed, and what the output now is
//!
//! This was the last `rts-codegen-new` entry point that dumped anything, and it
//! dumped that engine's Cranelift IR — a different representation from the one
//! `rts run` has compiled since the cutover, so the command answered about an
//! engine the user was not running. It now prints `rts_cranelift::ir`, which is
//! what this engine emits and what an optimization here changes.
//!
//! It is NOT Cranelift's `.clif`. That form only exists inside `lower/`, which
//! is the one module allowed to touch the code generator (the machine's rule 1)
//! — and it exists after every decision this engine makes has already been
//! taken, which is the wrong side of the question for reading an optimization.
//!
//! Printed to stdout, not stderr: the previous form went to stderr, so
//! `rts ir x.ts > dump.txt` wrote an empty file.

use std::path::PathBuf;

use anyhow::{Result, anyhow};

use crate::compile_options::CompileOptions;

pub fn command(input: Option<String>, _options: CompileOptions) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts ir <input.ts | inline-source>"))?;
    let path = PathBuf::from(&input);
    let text = if path.exists() {
        crate::cli::new_engine::describe_path(&path)
    } else if input.ends_with(".ts") || input.ends_with(".js") {
        return Err(anyhow!("input file not found: {}", path.display()));
    } else {
        // Not a file on disk and not named like one — treat as an inline snippet
        // (relative imports are not available in this form, like `eval`).
        crate::cli::new_engine::describe_source(&input)
    };
    print!("{}", text.map_err(|e| anyhow!("{e}"))?);
    Ok(())
}
