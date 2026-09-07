//! `rts prove` — what the closed world proved about a program, and where it
//! gave up.
//!
//! # Why this is a command and not a flag on `rts ir`
//!
//! They answer different questions and a reader wants one at a time. `rts ir`
//! shows what was emitted, which is the right form when the question is *what
//! does this compile to*. This answers *where did the proofs stop*, which is a
//! summary over the same IR and is unreadable if it arrives inside a few
//! thousand instructions.
//!
//! Accepts the same two inputs `rts ir` does: a file path, or an inline source
//! snippet with no disk imports.
//!
//! Printed to stdout, so `rts prove x.ts > before.txt` is half of a comparison.
//! That comparison is what the report is FOR — the counts are built to be
//! diffed across a change to the same program, and are deliberately bad at
//! ranking two different programs.

use std::path::PathBuf;

use anyhow::{Result, anyhow};

use crate::compile_options::CompileOptions;

pub fn command(input: Option<String>, _options: CompileOptions) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts prove <input.ts | inline-source>"))?;
    let path = PathBuf::from(&input);
    let text = if path.exists() {
        rts_host::prove::prove_path(&path).map_err(|e| anyhow!("{e:?}"))
    } else if input.ends_with(".ts") || input.ends_with(".js") {
        return Err(anyhow!("input file not found: {}", path.display()));
    } else {
        rts_host::prove::prove_source(&input).map_err(|e| anyhow!("{e:?}"))
    };
    print!("{}", text?);
    Ok(())
}
