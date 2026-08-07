//! Runs one file through the NEW engine, for the cross-runtime harness.
//!
//! A separate process on purpose. A fixture that reaches a nested borrow aborts,
//! and an abort cannot be caught — so an in-process harness would die on the
//! first one and report nothing about the other seven hundred. Here the abort
//! kills a child, and the parent records it as the result it is.
fn main() {
    let path = std::env::args().nth(1).expect("a fixture path");
    let mut program = match rts_host_rwk::compile_graph(std::path::Path::new(&path)) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("RTS-COMPILE-ERROR: {error:?}");
            std::process::exit(2);
        }
    };
    program.run();
}
