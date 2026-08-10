//! The shared way `rts run` and `rts test` reach the NEW engine
//! (`rts-host-rwk` + `rts-cranelift` + `rts-core-rwk`), after the cutover.
//!
//! `rts compile`, `rts ir` and `rts emit-types` do NOT use this module — they
//! stay on `rts-codegen-new` (see the comment at each of those commands) until
//! AOT gets an object-emission path and a runtime archive in the new engine.
//!
//! This is deliberately a thin restatement of
//! `crates/rts-host-rwk/examples/suite_run.rs` and `run_fixture.rs`, which are
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
fn imports_a_file(source: &str) -> bool {
    source.contains("from \"./")
        || source.contains("from \"../")
        || source.contains("from './")
        || source.contains("from '../")
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
/// This is not a convenience — it is load-bearing. `rts_std_rwk::test`'s
/// record is `thread_local!`: a caller that ran the program on this thread
/// and then read `rts_std_rwk::test::record()` back on the CALLING thread
/// would always read an empty record, silently reporting "0 tests" for every
/// file. `after` is the hook that lets `rts test` read the record where it
/// was actually written.
pub fn run_path_and<T: Send + 'static>(
    path: &Path,
    after: impl FnOnce(Result<(), String>) -> T + Send + 'static,
) -> T {
    let path = path.to_path_buf();
    let handle = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || {
            let result = run_path_inner(&path);
            after(result)
        })
        .expect("a thread to run the new engine on");
    handle.join().expect("the run thread not to panic")
}

fn run_path_inner(path: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("unreadable: {} ({e})", path.display()))?;
    let compiled = if imports_a_file(&source) {
        rts_host_rwk::compile_graph(path)
    } else {
        rts_host_rwk::compile(&source)
    };
    let mut program = compiled.map_err(|e| format!("{e:?}"))?;
    program.run();
    Ok(())
}
