//! A program that cannot reach the pickle registers nothing for it.
//!
//! Every registration is a runtime call at startup, and a program that never
//! imports `rts:serde` was paying one per top-level function and per class —
//! about 30 µs each in release (2026-09-18), hundreds of milliseconds for a
//! bundle. `emit/serde_names.rs` now decides once per compilation whether any
//! module can reach the pickle, and these tests pin the three shapes of that
//! decision by COUNTING `__rts_serde_declare` calls in the emitted IR: a
//! program with no route emits none; a route in ANY module registers EVERY
//! module; a route the compiler cannot see through — a computed `require(x)`,
//! an `eval` — counts as a route.

use std::path::PathBuf;

/// How many `__rts_serde_declare` calls an IR text holds.
///
/// The legend at the top of a dump names every callee the program uses, so a
/// program that never calls the entry point has no legend line for it either —
/// which is the zero the first test asserts, rather than a symbol found and
/// called nowhere.
fn declarations(ir: &str) -> usize {
    let Some(id) = ir
        .lines()
        .find(|line| line.starts_with(';') && line.ends_with("__rts_serde_declare"))
        .and_then(|line| line.split_whitespace().nth(1))
    else {
        return 0;
    };
    ir.lines()
        .filter(|line| {
            line.split("Call { callee: ")
                .nth(1)
                .and_then(|rest| rest.split(',').next())
                .is_some_and(|callee| callee == id)
        })
        .count()
}

fn of_source(source: &str) -> usize {
    declarations(&rts_host::describe::describe_source(source).expect("compiles"))
}

/// Writes a graph of files into a directory of its own and answers the entry —
/// the last file. Named after the test rather than randomised, so a failing
/// run leaves something a person can re-compile by hand.
fn graph(named: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("rts-serde-declare-gate")
        .join(named);
    std::fs::create_dir_all(&dir).expect("a directory to write the graph into");
    let mut entry = PathBuf::new();
    for (name, source) in files {
        let path = dir.join(name);
        std::fs::write(&path, source).expect("a file to write");
        entry = path;
    }
    entry
}

fn of_graph(named: &str, files: &[(&str, &str)]) -> usize {
    let entry = graph(named, files);
    declarations(&rts_host::describe::describe_path(&entry).expect("compiles"))
}

const DECLARES_THREE: &str = "function f(x) { return x; }\nfunction g() {}\nclass Point { constructor(x) { this.x = x; } }\nconsole.log(f(1), g(), new Point(2).x);\n";

#[test]
fn a_program_that_cannot_reach_the_pickle_registers_nothing() {
    assert_eq!(
        of_source(DECLARES_THREE),
        0,
        "two functions and a class, and no route to rts:serde"
    );
}

#[test]
fn a_static_import_of_the_pickle_registers_every_declaration() {
    let source = format!("import {{ serialize }} from \"rts:serde\";\n{DECLARES_THREE}");
    assert_eq!(of_source(&source), 3);
}

#[test]
fn node_v8_is_the_same_pickle_and_registers_too() {
    let source = format!("import {{ serialize }} from \"node:v8\";\n{DECLARES_THREE}");
    assert_eq!(of_source(&source), 3);
}

#[test]
fn an_import_in_another_module_registers_every_module_of_the_program() {
    // The module that declares the class never mentions the pickle; the entry
    // that imports it does. Both register, because the class is what the entry
    // serializes.
    let count = of_graph(
        "other-module",
        &[
            (
                "model.ts",
                "export function make(x) { return new Point(x); }\nexport class Point { constructor(x) { this.x = x; } }\n",
            ),
            (
                "main.ts",
                "import { serialize } from \"rts:serde\";\nimport { make } from \"./model\";\nfunction show(p) { return p.x; }\nconsole.log(show(make(1)), serialize(make(2)).length);\n",
            ),
        ],
    );
    assert_eq!(
        count, 3,
        "make, Point and show — the two of model.ts included"
    );
}

#[test]
fn a_graph_with_no_route_registers_nothing_in_any_module() {
    let count = of_graph(
        "no-route",
        &[
            (
                "model.ts",
                "export class Point { constructor(x) { this.x = x; } }\n",
            ),
            (
                "main.ts",
                "import { Point } from \"./model\";\nfunction show(p) { return p.x; }\nconsole.log(show(new Point(1)));\n",
            ),
        ],
    );
    assert_eq!(count, 0);
}

#[test]
fn a_computed_require_can_reach_anything_and_so_registers() {
    // `require(x)` resolves at run time against the table every declared
    // module is in, `rts:serde` included, so the compiler cannot rule it out.
    let source =
        format!("const name = process.argv[2];\nconst m = require(name);\n{DECLARES_THREE}");
    assert_eq!(of_source(&source), 3);
}

#[test]
fn a_literal_require_of_something_else_is_not_a_route() {
    let source = format!("const fs = require(\"node:fs\");\n{DECLARES_THREE}");
    assert_eq!(of_source(&source), 0);
}

#[test]
fn eval_can_write_an_import_and_so_registers() {
    let source = format!("{DECLARES_THREE}eval(\"1\");\n");
    assert_eq!(of_source(&source), 3);
}

#[test]
fn a_call_of_function_registers_but_its_prototype_does_not() {
    let called = format!("{DECLARES_THREE}const h = new Function(\"return 1\");\n");
    assert_eq!(
        of_source(&called),
        3,
        "`new Function` compiles code that may import the pickle"
    );
    let read = format!("{DECLARES_THREE}const bind = Function.prototype.bind;\n");
    assert_eq!(
        of_source(&read),
        0,
        "reading `Function.prototype` compiles nothing"
    );
}
