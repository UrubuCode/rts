//! Integration tests: compile each `.ts` fixture and compare stdout to `.out`.
//!
//! Each test compiles the fixture via the RTS pipeline, links against the
//! runtime support objects, runs the resulting binary, and asserts its stdout
//! matches the adjacent `.out` file byte-for-byte.
//!
//! Requires a pre-built `target/release/rts` binary (or `target/debug/rts`
//! as fallback). Tests are skipped if no binary is found.

use std::path::{Path, PathBuf};
use std::process::Command;

fn rts_binary() -> Option<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let release = manifest.join("target/release/rts.exe");
    let debug = manifest.join("target/debug/rts.exe");
    // Non-Windows paths
    let release_nix = manifest.join("target/release/rts");
    let debug_nix = manifest.join("target/debug/rts");

    for p in [release, debug, release_nix, debug_nix] {
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn run_fixture(name: &str) {
    let Some(rts) = rts_binary() else {
        eprintln!("skipping {name}: no rts binary found");
        return;
    };

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ts_path = manifest.join(format!("tests/fixtures/{name}.ts"));
    let expected_path = manifest.join(format!("tests/fixtures/{name}.out"));

    let expected = std::fs::read_to_string(&expected_path)
        .unwrap_or_else(|_| panic!("missing expected output file: {}", expected_path.display()));

    let output = Command::new(&rts)
        .args(["run", ts_path.to_str().unwrap()])
        .output()
        .unwrap_or_else(|e| panic!("failed to run rts for {name}: {e}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "fixture `{name}` exited with status {}\nstderr: {stderr}",
        output.status,
    );

    assert_eq!(
        stdout.as_ref(),
        expected,
        "fixture `{name}` stdout mismatch\nstderr: {stderr}",
    );
}

#[test]
fn fixture_empty() {
    run_fixture("empty");
}

#[test]
fn fixture_print() {
    run_fixture("print");
}

#[test]
fn fixture_arithmetic() {
    run_fixture("arithmetic");
}

#[test]
fn fixture_if_else() {
    run_fixture("if_else");
}

#[test]
fn fixture_while_loop() {
    run_fixture("while_loop");
}

#[test]
fn fixture_functions() {
    run_fixture("functions");
}

#[test]
fn fixture_string_concat() {
    run_fixture("string_concat");
}

#[test]
fn fixture_template_literals() {
    run_fixture("template_literals");
}

#[test]
fn fixture_let_const_var() {
    run_fixture("let_const_var");
}

#[test]
fn fixture_function_expressions() {
    run_fixture("function_expressions");
}

#[test]
fn fixture_arrow_functions() {
    run_fixture("arrow_functions");
}

#[test]
fn fixture_bitwise_ops() {
    run_fixture("bitwise_ops");
}

#[test]
fn fixture_ternary() {
    run_fixture("ternary");
}

#[test]
fn fixture_f64_modulo() {
    run_fixture("f64_modulo");
}

#[test]
fn fixture_switch_jump_table() {
    run_fixture("switch_jump_table");
}

#[test]
fn fixture_exponentiation() {
    run_fixture("exponentiation");
}

#[test]
fn fixture_compound_assign() {
    run_fixture("compound_assign");
}

#[test]
fn fixture_typeof_void_delete() {
    run_fixture("typeof_void_delete");
}

#[test]
fn fixture_nullish_optional() {
    run_fixture("nullish_optional");
}

#[test]
fn fixture_try_catch() {
    run_fixture("try_catch");
}

#[test]
fn fixture_tail_call() {
    run_fixture("tail_call");
}

#[test]
fn fixture_first_class_functions() {
    run_fixture("first_class_functions");
}

#[test]
fn fixture_object_array_literals() {
    run_fixture("object_array_literals");
}

#[test]
fn fixture_for_of() {
    run_fixture("for_of");
}

#[test]
fn fixture_string_eq() {
    run_fixture("string_eq");
}
