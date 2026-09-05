//! `rts run <input.ts>` — compile + execute via the NEW engine (Cranelift JIT).

use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};

use crate::compile_options::CompileOptions;

pub fn command(input: Option<String>, _options: CompileOptions) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts run <input.ts|input.html>"))?;
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

    // `.html` needs no TypeScript at all, same as `rts compile pagina.html` —
    // `cli::html_entry::for_run` writes the `app.ts` window loop in its
    // place, reading the page from disk at run time (like
    // `examples/view.ts` does today) rather than embedding it, since nothing
    // here ships anywhere else.
    //
    // `run_path` on a MIRRORED file, not `run_source`: `run_source` compiles
    // and runs on a freshly SPAWNED thread (`new_engine::on_a_deep_thread`),
    // and winit panics building an event loop off the main thread — the exact
    // trap `run_path_and`'s own comment names, and the reason `run_path`
    // itself no longer spawns one. `write_shell` mirrors the generated
    // program the same way [`crate::url_entry::fetch_program`] mirrors a URL
    // entry, so a `.html` entry's window opens on the calling thread exactly
    // like an ordinary `.ts` one does.
    if crate::cli::html_entry::is_html(&input_path) {
        let html = std::fs::read_to_string(&input_path)
            .with_context(|| format!("read {}", input_path.display()))?;
        let source = crate::cli::html_entry::for_run(&html, &input_path);
        let shell_path = crate::cli::html_entry::write_shell(&source, &input_path)
            .with_context(|| format!("write the generated shell for {}", input_path.display()))?;
        return crate::cli::new_engine::run_path(&shell_path)
            .map_err(|e| anyhow!("{e}"))
            .with_context(|| format!("run of {} failed", input_path.display()));
    }

    // Cutover: execute through the NEW engine (`rts-host` over
    // `rts-cranelift` + `rts-core`). `new_engine::run_path` resolves the
    // relative-import graph from the entry (compiling as a GRAPH when one is
    // found — see its module doc), lowers to Cranelift and runs it in-memory
    // (JIT), on a thread with the stack budget the emitter needs. An uncaught
    // exception inside the program is reported and ends the process from
    // inside `Compiled::run` itself (see `rts-host::run`), the same way a
    // real runtime ends on one — it is not swallowed here.
    crate::cli::new_engine::run_path(&input_path)
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
    // The NEW engine, like `rts run`. It was the old one until now, so the same
    // snippet could answer differently depending on whether it was typed at
    // `-e` or saved to a file first — which is the worst shape a difference
    // between two engines can take, because nothing in the command says it.
    crate::cli::new_engine::run_source(&source)
        .map_err(|e| anyhow!("{e}"))
        .context("eval falhou")?;
    Ok(())
}

fn is_stdin_tty() -> bool {
    std::io::stdin().is_terminal()
}
