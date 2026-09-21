//! The shared way `rts run` and `rts test` reach the NEW engine
//! (`rts-host` + `rts-cranelift` + `rts-core`), after the cutover.
//!
//! `rts emit-types` does NOT use this module — it stays on `rts-codegen-new`,
//! which is the last thing that does (see the comment at that command).
//!
//! This is deliberately a thin restatement of
//! `crates/rts-host/examples/suite_run.rs` and `run_fixture.rs`, which are
//! the reference implementations for running a program on this engine. Two
//! decisions are carried over rather than re-derived:
//!
//! - A file that imports another — relatively (`./x`, `../x`) or through a
//!   `tsconfig.json` alias (`@/x`) — must be compiled as a GRAPH
//!   (`compile_graph`), never as a single file — such an import compiled
//!   alone binds to nothing, which reports as "ran and failed every
//!   assertion" rather than the missing-import problem it actually is.
//!   WHICH files a specifier names is `rts_host::names_any_file`'s answer and
//!   is not re-derived here; see [`imports_a_file`].
//! - The compile-and-run has to happen on a thread with a 64 MB stack: the
//!   emitter recurses with the shape of the expression it lowers, and a chain
//!   of about a hundred `+` overflows Windows' 1 MB default main-thread stack
//!   AT COMPILE TIME. `cargo test`'s harness threads hide this (they get more
//!   stack), which is exactly the trap — the same file compiles under one
//!   measuring instrument and kills the process under another.

use std::path::Path;

/// Same budget `suite_run`/`run_fixture` use, for the same reason.
const STACK: usize = 64 * 1024 * 1024;

/// Whether `source` names another file — and therefore must be compiled as a
/// GRAPH — asked of `rts-host`, which owns the answer.
///
/// # Why the substring test is gone
///
/// It used to be four `contains` calls for `from "./"` and its three
/// spellings, described here as "the same substring test `suite_run.rs`
/// uses". That was a second implementation of "does this name a file", and it
/// drifted the moment a `tsconfig.json` alias became a way to name one: an
/// aliased specifier contains none of the four, so a program whose EVERY
/// import is an alias answered "no" here, was compiled alone, and died at run
/// time on `cannot resolve module "@/…" — nothing registered that specifier`.
/// One relative import beside it hid the defect, which is why the whole suite
/// missed it.
///
/// [`rts_host::names_any_file`] is the loader's own resolution, aliases
/// included. A parse failure there answers `false` and the single-file path
/// reports the parse error itself, which is the same error by a shorter route.
///
/// The arms BELOW are kept, because they answer a different question. None of
/// them is about naming a file the loader must read; they are about a module
/// needing a SPECIFIER at all, which only the graph path gives it.
pub(super) fn imports_a_file(source: &str, entry: &Path) -> bool {
    rts_host::names_any_file(source, entry).unwrap_or(false)
        // A COMPUTED dynamic `import(name)` names no file the resolution above
        // could have found — nothing static was there to resolve — but the
        // module still needs a specifier to ask from, so it needs the graph.
        || source.contains("import(")
        // `import.meta` names no file, and is here for the other half of what
        // the graph gives: a module compiled alone has no SPECIFIER, and
        // `import.meta` is refused without one. What remains a substring test
        // is only this half — the thing that would end it is compiling every
        // file as a graph of one, and that is a change to measure on its own
        // rather than to smuggle in beside a feature.
        || source.contains("import.meta")
        // And CommonJS, for BOTH halves at once. A `require("./x")` names a file
        // exactly as an `import` does, and the four names beside it need what
        // `import.meta` needs: a module compiled alone has no specifier, and
        // `require`, `module`, `exports` and `__filename` are all bound from one.
        // Without this line a file that only writes `module.exports = …` was
        // compiled as a script, got no binding, and died on `module is not
        // defined` — the same fault as `import.meta`, through a third spelling.
        || source.contains("require(")
        || source.contains("module.exports")
        || source.contains("exports.")
        || source.contains("__filename")
        || source.contains("__dirname")
}

/// Compiles and runs `path` through the new engine, on a thread with the
/// stack budget the emitter needs. Returns the `{error:?}` rendering of a
/// `HostError` on failure — one line, so a caller printing it (or a driver
/// reading it) is never confused by a multi-line debug rendering.
pub fn run_path(path: &Path) -> Result<(), String> {
    run_path_and(path, |result| result)
}

/// Same as [`run_path`], but `after` runs on the SAME thread as the compile
/// and the run, immediately afterward, and its result is handed back out.
///
/// This is not a convenience — it is load-bearing. `rts_std::test`'s
/// record is `thread_local!`: a caller that ran the program on this thread
/// and then read `rts_std::test::record()` back on the CALLING thread
/// would always read an empty record, silently reporting "0 tests" for every
/// file. `after` is the hook that lets `rts test` read the record where it
/// was actually written.
pub fn run_path_and<T: Send + 'static>(
    path: &Path,
    after: impl FnOnce(Result<(), String>) -> T + Send + 'static,
) -> T {
    let path = path.to_path_buf();
    // NA THREAD PRINCIPAL, e isso e o que deixa um programa abrir janela.
    //
    // Isto criava uma thread com `STACK` de pilha, e a razao era boa: recursao
    // de JS estoura a pilha padrao do Windows. So que o `winit` entra em panico
    // ao criar o event loop fora da principal — entao todo programa com janela
    // morria antes do primeiro frame, e a UI virou um exemplo separado
    // (`ui_fixture`) em vez do caminho normal.
    //
    // Os dois requisitos so se excluiam enquanto a principal tinha ~1 MiB. O
    // `.cargo/config.toml` agora a linka com `/STACK:67108864` — os mesmos 64
    // MiB que esta thread pedia —, entao rodar aqui tem a profundidade E a
    // janela. A thread deixou de comprar alguma coisa.
    //
    // O `after` continua rodando na MESMA thread que o programa, que e o
    // contrato que este par existe para manter: o registro de `rts_std::test` e
    // `thread_local`, e le-lo noutra thread reportaria "0 tests" em silencio.
    // Rodar tudo na principal preserva isso por construcao, em vez de por
    // combinacao.
    let result = run_path_inner(&path);
    after(result)
}

/// Compiles and runs source text through the new engine, on the same thread
/// budget [`run_path`] uses.
///
/// No graph: text has no directory, so a relative import has nothing to be
/// relative TO. That is why this is a separate function rather than a flag on
/// `run_path` — a caller cannot ask for something the input cannot answer.
pub fn run_source(source: &str) -> Result<(), String> {
    on_a_deep_thread(source.to_owned(), |source| {
        let mut program = rts_host::compile(&source).map_err(|e| format!("{e:?}"))?;
        program.run();
        Ok(())
    })
}

/// The IR of a program, as text, without running it. See
/// [`rts_host::describe`].
///
/// On the deep thread for the same reason the run is: emission is what recurses
/// with the shape of the expression, and a dump emits everything a run does.
pub fn describe_path(path: &Path) -> Result<String, String> {
    let path = path.to_path_buf();
    on_a_deep_thread(path, |path| {
        rts_host::describe::describe_path(&path).map_err(|e| format!("{e:?}"))
    })
}

/// The IR of source text, as text. See [`run_source`] for why text and a path
/// are two functions.
pub fn describe_source(source: &str) -> Result<String, String> {
    on_a_deep_thread(source.to_owned(), |source| {
        rts_host::describe::describe_source(&source).map_err(|e| format!("{e:?}"))
    })
}

fn on_a_deep_thread<I: Send + 'static, T: Send + 'static>(
    input: I,
    work: impl FnOnce(I) -> T + Send + 'static,
) -> T {
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || work(input))
        .expect("a thread to run the new engine on")
        .join()
        .expect("the engine thread not to panic")
}

fn run_path_inner(path: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("unreadable: {} ({e})", path.display()))?;
    let compiled = if imports_a_file(&source, path) {
        rts_host::compile_graph(path)
    } else {
        rts_host::compile(&source)
    };
    let mut program = compiled.map_err(|e| format!("{e:?}"))?;
    program.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    fn fixture(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rts_cli_decide_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        for (relative, source) in files {
            let path = dir.join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            let mut file = std::fs::File::create(&path).expect("a fixture file");
            file.write_all(source.as_bytes()).expect("written");
        }
        dir
    }

    /// The defect task 9 fixes. A program whose ONLY import is an alias must
    /// be compiled as a graph; the substring test that used to live here said
    /// it named no file, and the program died at run time on
    /// `cannot resolve module "@/…" — nothing registered that specifier`.
    #[test]
    fn um_import_so_por_alias_decide_grafo() {
        let dir = fixture(
            "alias",
            &[
                ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
                ("src/compat/io.ts", "export const value = 7;\n"),
                ("app.ts", "import { value } from \"@/compat/io.ts\";\nconsole.log(value);\n"),
            ],
        );
        let entry = dir.join("app.ts");
        let source = std::fs::read_to_string(&entry).expect("le a fixture");
        assert!(
            super::imports_a_file(&source, &entry),
            "um alias nomeia um ficheiro exatamente como `./` nomeia"
        );
    }

    /// The control, and the no-config guarantee: with no `tsconfig.json` the
    /// same alias names nothing, and a program that imports only host modules
    /// is still compiled alone.
    #[test]
    fn sem_tsconfig_e_sem_import_de_ficheiro_decide_ficheiro_unico() {
        let dir = fixture(
            "semconfig",
            &[("app.ts", "import { test } from \"rts:test\";\ntest(\"x\", () => {});\n")],
        );
        let entry = dir.join("app.ts");
        let source = std::fs::read_to_string(&entry).expect("le a fixture");
        assert!(
            !super::imports_a_file(&source, &entry),
            "`rts:test` e respondido pelo runtime e nao nomeia ficheiro"
        );
    }

    /// The arms that were KEPT: they answer "does this module need a
    /// specifier", not "does it name a file", and no resolution replaces them.
    #[test]
    fn import_meta_e_commonjs_continuam_a_decidir_grafo() {
        let dir = fixture("meta", &[("app.ts", "console.log(import.meta.url);\n")]);
        let entry = dir.join("app.ts");
        assert!(super::imports_a_file("console.log(import.meta.url);\n", &entry));
        assert!(super::imports_a_file("module.exports = 1;\n", &entry));
        assert!(super::imports_a_file("const x = require(\"node:fs\");\n", &entry));
    }
}
