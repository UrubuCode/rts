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
//! - A file that imports another (`./x`, `../x`) must be compiled as a GRAPH
//!   (`compile_graph`), never as a single file — a relative import compiled
//!   alone binds to nothing, which reports as "ran and failed every
//!   assertion" rather than the missing-import problem it actually is.
//! - The compile-and-run has to happen on a thread with a 64 MB stack: the
//!   emitter recurses with the shape of the expression it lowers, and a chain
//!   of about a hundred `+` overflows Windows' 1 MB default main-thread stack
//!   AT COMPILE TIME. `cargo test`'s harness threads hide this (they get more
//!   stack), which is exactly the trap — the same file compiles under one
//!   measuring instrument and kills the process under another.

use std::path::Path;

/// Same budget `suite_run`/`run_fixture` use, for the same reason.
const STACK: usize = 64 * 1024 * 1024;

/// Whether `source` names another file by a relative specifier — the same
/// substring test `suite_run.rs` uses. `node:`/`rts:` specifiers are resolved
/// by the runtime; only `./`/`../` names a file the loader has to read.
pub(super) fn imports_a_file(source: &str) -> bool {
    source.contains("from \"./")
        || source.contains("from \"../")
        || source.contains("from './")
        || source.contains("from '../")
        // A dynamic `import("./x")` names a file too, and no `from` appears in
        // it — so the test above answered "no imports" and the module was
        // compiled alone, which is the failure this file's own header warns
        // about arriving through the other spelling.
        || source.contains("import(")
        // `import.meta` names no file, and is here for the other half of what
        // the graph gives: a module compiled alone has no SPECIFIER, and
        // `import.meta` is refused without one. A substring test decides how a
        // file is compiled, which is the shape this whole function is — the
        // thing that ends it is compiling every file as a graph of one, and
        // that is a change to measure on its own rather than to smuggle in
        // beside a feature.
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
    let compiled = if imports_a_file(&source) {
        rts_host::compile_graph(path)
    } else {
        rts_host::compile(&source)
    };
    let mut program = compiled.map_err(|e| format!("{e:?}"))?;
    program.run();
    Ok(())
}
