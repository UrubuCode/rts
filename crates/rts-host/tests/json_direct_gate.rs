//! `JSON.stringify(x)` is called by its entry point exactly where the proof holds.
//!
//! `tests/json_direct_call.test.ts` pins what the call ANSWERS, and passes on a
//! binary from before the entry point existed — which is what a semantics test
//! has to do, and also why it cannot say whether the substitution happens at
//! all. A guard that refused everything would pass it. So this counts
//! `__rts_json_stringify` and `__rts_json_parse` in the emitted IR, in both
//! directions: present where `primordial::only_a_base` holds, absent in every
//! program where the name could mean something else by the time it runs.

use std::path::PathBuf;

/// How many calls to `symbol` an IR text holds. `serde_declare_gate.rs` has the
/// reasoning for reading the legend first.
fn calls(ir: &str, symbol: &str) -> usize {
    let Some(id) = ir
        .lines()
        .find(|line| line.starts_with(';') && line.ends_with(symbol))
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

/// `(stringify, parse)` calls by entry point in one source.
fn direct(source: &str) -> (usize, usize) {
    let ir = rts_host::describe::describe_source(source).expect("compiles");
    (
        calls(&ir, "__rts_json_stringify"),
        calls(&ir, "__rts_json_parse"),
    )
}

#[test]
fn an_undisturbed_program_calls_both_by_their_entry_points() {
    let source = "const o = { a: 1 };\nconsole.log(JSON.stringify(o), JSON.parse(\"[1]\"));\n";
    assert_eq!(direct(source), (1, 1));
}

#[test]
fn a_call_inside_a_nested_function_and_a_class_method_counts_too() {
    let source = "function f(v) { return JSON.stringify(v); }\n\
                  class C { m(t) { return JSON.parse(t); } }\n\
                  console.log(f(1), new C().m(\"2\"));\n";
    // AT LEAST one, not exactly: `f` is small enough for `emit/inline.rs` to
    // emit at its call site too, so its `stringify` is counted where it was
    // declared and again where it was substituted. Both are the entry point,
    // which is the claim; how many copies the inliner makes is not this test's.
    let (stringify, parse) = direct(source);
    assert!(
        stringify >= 1,
        "a nested function's call is direct: {stringify}"
    );
    assert_eq!(parse, 1, "and so is a class method's");
}

#[test]
fn a_replacer_an_indentation_a_reviver_and_a_spread_stay_the_ordinary_call() {
    let source = "const parts = [\"[1]\"];\n\
                  console.log(JSON.stringify({}, null), JSON.stringify({}, null, 2));\n\
                  console.log(JSON.parse(\"1\", (k, v) => v), JSON.parse(...parts), JSON.stringify());\n";
    assert_eq!(direct(source), (0, 0));
}

#[test]
fn a_write_through_the_name_ends_the_proof_for_the_whole_program() {
    let source = "console.log(JSON.stringify(1));\nJSON.stringify = () => \"mine\";\n";
    assert_eq!(direct(source), (0, 0));
}

#[test]
fn a_copy_of_the_object_ends_it_too_because_a_write_may_go_through_the_copy() {
    // The case `Math`'s proof accepts and this one does not.
    let alias = "const held = JSON;\nconsole.log(JSON.stringify(1));\n";
    assert_eq!(direct(alias), (0, 0));
    let argument = "Object.defineProperty(JSON, \"parse\", { value: () => 1 });\nconsole.log(JSON.parse(\"2\"));\n";
    assert_eq!(direct(argument), (0, 0));
}

#[test]
fn reaching_for_eval_or_globalthis_ends_every_proof_about_a_name() {
    assert_eq!(
        direct("eval(\"1\");\nconsole.log(JSON.stringify(1));\n"),
        (0, 0)
    );
    assert_eq!(
        direct("globalThis.x = 1;\nconsole.log(JSON.parse(\"1\"));\n"),
        (0, 0)
    );
}

#[test]
fn a_binding_named_json_is_the_scopes_and_only_that_call_is_left_alone() {
    let source = "function through(JSON) { return JSON.stringify(1); }\n\
                  console.log(through({ stringify: () => \"mine\" }), JSON.stringify(2));\n";
    assert_eq!(
        direct(source),
        (1, 0),
        "the parameter's call is ordinary, the global's is direct"
    );
}

#[test]
fn reading_the_function_without_calling_it_disturbs_nothing() {
    // `JSON.stringify` as a VALUE is still a member of the base: the function
    // it names is the primordial either way, and nothing here can replace it.
    let source = "const s = JSON.stringify;\nconsole.log(s(1), JSON.stringify(2));\n";
    assert_eq!(direct(source), (1, 0));
}

/// Writes a graph into a directory named after the test and answers the entry,
/// which is the last file.
fn graph(named: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("rts-json-direct-gate")
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

#[test]
fn one_module_holding_a_copy_ends_the_proof_in_every_module() {
    // The proof is about the PROGRAM. Asked per module, `main.ts` below would
    // call the entry point while `patch.ts` replaces the function it names.
    let entry = graph(
        "copy-in-another-module",
        &[
            (
                "patch.ts",
                "const held = JSON;\nexport function patch() { held.stringify = () => \"patched\"; }\n",
            ),
            (
                "main.ts",
                "import { patch } from \"./patch.ts\";\npatch();\nconsole.log(JSON.stringify({ a: 1 }));\n",
            ),
        ],
    );
    let ir = rts_host::describe::describe_path(&entry).expect("compiles");
    assert_eq!(calls(&ir, "__rts_json_stringify"), 0);
}
