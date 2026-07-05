//! `rts run <input.ts>` — compile + execute via the NEW engine (Cranelift JIT).

use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};

use crate::compile_options::CompileOptions;

pub fn command(input: Option<String>, _options: CompileOptions) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts run <input.ts>"))?;
    // An http(s) URL entry: mirror it (plus its relative-import graph) into the
    // system temp dir and run the LOCAL copy through the normal disk pipeline.
    let input_path = if crate::url_entry::is_url(&input) {
        crate::url_entry::fetch_program(&input)?
    } else {
        PathBuf::from(&input)
    };
    if !input_path.exists() {
        return Err(anyhow!("input file not found: {}", input_path.display()));
    }

    // Load .env from the project directory before executing.
    if let Ok(abs) = input_path.canonicalize() {
        if let Some(dir) = abs.parent() {
            crate::dotenv::load_from_dir(dir);
        }
    }

    // Cutover: execute through the NEW engine. `run_path` resolves the relative-
    // import graph from the entry, lowers HIR→Cranelift (single path) and runs it
    // in-memory (JIT). Builtins (`rts:*`) resolve via the new engine's registry.
    rts_codegen_new::front::run::run_path(&input_path)
        .map_err(|e| anyhow!("{e}"))
        .with_context(|| format!("run of {} failed", input_path.display()))?;
    Ok(())
}

/// `rts eval "<source>"` / `rts -e "<source>"` — compile + execute inline TS via
/// the new engine. Relative imports (`./mod`) are not resolved — builtins only.
pub fn eval_command(input: Option<String>, _options: CompileOptions) -> Result<()> {
    let source = match input {
        Some(s) => s,
        None => {
            if is_stdin_tty() {
                return Err(anyhow!(
                    "usage: rts eval \"<source>\" ou rts -e \"<source>\"\n\
                     (alternativa: 'echo ... | rts -e' para ler de stdin)"
                ));
            }
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("falha ao ler stdin")?;
            if buf.trim().is_empty() {
                return Err(anyhow!("stdin vazio"));
            }
            buf
        }
    };
    rts_codegen_new::front::run::run_source(&source)
        .map_err(|e| anyhow!("{e}"))
        .context("eval falhou")?;
    Ok(())
}

fn is_stdin_tty() -> bool {
    std::io::stdin().is_terminal()
}
