//! `rts run-new <file>` — run a single .ts file through the NEW engine
//! (rts-codegen-new), used by the cross-runtime check to measure the redesign.
//! Bails (Unsupported) on any construct outside the new engine's implemented
//! subset: stdout stays empty and the process exits non-zero (the cross-runtime
//! script counts that as an RTS error/bail).

use std::path::PathBuf;

use anyhow::anyhow;

pub fn command(input: Option<String>) -> anyhow::Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts run-new <input.ts>"))?;
    let input_path = PathBuf::from(&input);
    if !input_path.exists() {
        return Err(anyhow!("input file not found: {}", input_path.display()));
    }
    // `run_path` resolves the whole relative-import graph from the entry file's
    // directory (M1b) and runs the flattened multi-file program. A single-file
    // program with no imports flows through the same path unchanged. The eval/`-e`
    // path keeps using `run_source` (a string, no disk imports).
    match rts_codegen_new::front::run::run_path(&input_path) {
        Ok(()) => std::process::exit(0),
        Err(unsupported) => {
            eprintln!("error: {unsupported}");
            std::process::exit(1);
        }
    }
}
