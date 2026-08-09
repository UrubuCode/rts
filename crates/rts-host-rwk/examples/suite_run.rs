//! Runs ONE suite file and reports what it scored, for a driver to aggregate.
//!
//! # Why one file per process
//!
//! Two of the ways a suite file can fail take the whole process with them: an
//! uncaught exception ends it through `std::process::exit(1)` in `run.rs`, and a
//! program that loops never returns at all. A harness that ran all 818 in one
//! process would report whatever it had reached before the first of those and
//! call it the score — which is a number measured against a corpus quietly
//! smaller than claimed.
//!
//! So the aggregation lives in the driver, which spawns this per file with a
//! timeout and reads one line.
//!
//! # The line
//!
//! `<status> <passed> <failed> <path>`, where status is one of `ok`, `fail`,
//! `refused`, and a missing line at all means the process died or hung — which
//! the driver reports as its own outcome rather than guessing.

use std::path::PathBuf;

/// Why the work happens on a spawned thread rather than on `main`.
///
/// Windows gives the main thread of an executable 1 MB, and the emitter recurses
/// with the shape of the expression it is lowering — a chain of about a hundred
/// `+` overflows it while compiling. `cargo test` does not see this, because its
/// harness threads are given more, which is exactly the trap: the same file
/// compiles in one harness and kills the process in another, and the difference
/// is the measuring instrument rather than the engine.
///
/// So the stack is stated here. A file that still overflows this is reporting
/// something about the engine.
const STACK: usize = 64 * 1024 * 1024;

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(measure)
        .expect("a thread to measure on");
    let _ = handle.join();
}

fn measure() {
    let path = match std::env::args().nth(1) {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: suite_run <file.ts>");
            std::process::exit(2);
        }
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            println!("refused 0 0 {} unreadable: {error}", path.display());
            return;
        }
    };

    rts_std_rwk::test::reset();
    // A file that imports another is compiled as a GRAPH, and one that does not
    // is compiled on its own. Measuring everything with the single-file path
    // reported a file whose imports bound nothing as a file that ran and failed
    // every assertion — 14 of them in `module_imports.test.ts` alone, which is
    // about this measurement rather than about the engine.
    //
    // A relative specifier is the test: `node:` and `rts:` are resolved by the
    // runtime, and only `./` and `../` name another file the loader has to read.
    let imports_a_file = source.contains("from \"./")
        || source.contains("from \"../")
        || source.contains("from './")
        || source.contains("from '../");
    let compiled = match imports_a_file {
        true => rts_host_rwk::compile_graph(&path),
        false => rts_host_rwk::compile(&source),
    };
    let mut program = match compiled {
        Ok(program) => program,
        Err(error) => {
            // One line, so a driver reading line-by-line is never confused by a
            // multi-line debug rendering of an error.
            let why = format!("{error:?}").replace('\n', " ");
            println!("refused 0 0 {} {why}", path.display());
            return;
        }
    };
    program.run();

    let reported = rts_std_rwk::test::record();
    let failed: Vec<String> = reported
        .iter()
        .filter_map(|one| one.failure.clone().map(|why| format!("{}: {why}", one.name)))
        .collect();
    let passed = reported.len() - failed.len();

    // A file that registered nothing is NOT a pass. It compiled and ran and
    // asserted nothing, which is the shape of a file whose body threw before it
    // reached its first `test(...)` — reporting that as `ok` would count a
    // silent failure as a success.
    let status = match (reported.is_empty(), failed.is_empty()) {
        (true, _) => "empty",
        (false, true) => "ok",
        (false, false) => "fail",
    };
    println!(
        "{status} {passed} {} {} {}",
        failed.len(),
        path.display(),
        failed.join(" | ").replace('\n', " ")
    );
}
