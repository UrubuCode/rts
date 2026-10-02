//! An import CYCLE links, with CommonJS semantics — #2852.
//!
//! # What changed, and why the old answer was right for the old design
//!
//! Every module body used to run in a topological sweep before the program
//! started, and a cycle has no topological order — so the loader refused one by
//! name. A module now runs when something NAMES it, which is what Node does and
//! the only shape a cycle works in: the module being re-entered is already
//! registered, with a namespace to hand back half filled.
//!
//! Dropping the cycle edge WITHOUT that change was measured and was worse: the
//! order became `b, a`, and `b`'s `require("./a.js")` answered `cannot find
//! module` — a refusal at compile time traded for a throw in the middle of
//! execution. What makes it correct now is that every module is registered
//! before any body runs, so "already registered" no longer means "already ran".
//!
//! # The contract these pin, and the one case that separates the models
//!
//! `a_module_sees_what_was_published_before_the_require` is the decisive one. A
//! pre-ordered sweep and on-demand execution agree about a cycle whose `require`
//! sits on the first line — both answer `undefined` — so a fixture that only
//! tested that would pass for the wrong reason. A module that publishes BEFORE
//! requiring is seen by the module it requires, and only on-demand execution
//! gives that.
//!
//! Every expected value here was measured with `node` 22.

use std::io::Write;
use std::path::PathBuf;

fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rts_import_cycle_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, source) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
    }
    dir
}

/// Runs the entry as a graph and answers each test's verdict.
fn run(entry: &PathBuf) -> String {
    let source = std::fs::read_to_string(entry).expect("the fixture reads");
    assert!(
        rts_host::names_any_file(&source, entry).expect("it parses"),
        "the fixture must name a file, or this tests the single-file path"
    );
    rts_std::test::reset();
    let mut program = rts_host::compile_graph(entry).expect("a cycle links");
    program.run();
    rts_std::test::record()
        .iter()
        .map(|one| match &one.failure {
            Some(why) => format!("FAIL {}: {why}", one.name),
            None => format!("ok {}", one.name),
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The plain CommonJS cycle: `undefined` mid-flight, the real value afterwards.
///
/// Both halves matter and they are the same object: `module.exports` is filled
/// over time, so a function reading it LATER sees everything. `node` prints
/// `A`, `undefined`, `B` for this program.
#[test]
fn a_commonjs_cycle_answers_a_partial_object_mid_flight() {
    let dir = fixture(
        "partial",
        &[
            (
                "a.js",
                "const b = require(\"./b.js\");\n\
                 exports.fromA = \"A\";\n\
                 exports.whatBSaw = b.seenFromB;\n\
                 exports.late = () => b.fromB;\n",
            ),
            (
                "b.js",
                "const a = require(\"./a.js\");\n\
                 exports.seenFromB = typeof a.fromA;\n\
                 exports.fromB = \"B\";\n",
            ),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 const a: any = require(\"./a.js\");\n\
                 test(\"a published its own name\", () => expect(a.fromA).toBe(\"A\"));\n\
                 test(\"b saw a half built\", () => expect(a.whatBSaw).toBe(\"undefined\"));\n\
                 test(\"and a function reading later sees it whole\", () =>\n\
                   expect(a.late()).toBe(\"B\"));\n",
            ),
        ],
    );
    let report = run(&dir.join("app.ts"));
    assert!(!report.contains("FAIL"), "{report}");
    assert_eq!(report.matches("ok ").count(), 3, "{report}");
}

/// THE decisive one: what is published before the `require` is visible to the
/// module that `require` reaches.
///
/// A pre-ordered sweep cannot answer this. With the cycle edge dropped the order
/// is `b, a`, so `b` runs FIRST and sees nothing of `a`. On-demand execution
/// runs `b` from inside `a`, after `a` published — `node` answers
/// `published-before-require`.
#[test]
fn a_module_sees_what_was_published_before_the_require() {
    let dir = fixture(
        "ordering",
        &[
            (
                "a.js",
                "exports.early = \"published-before-require\";\n\
                 const b = require(\"./b.js\");\n\
                 exports.whatBSaw = b.sawEarly;\n",
            ),
            ("b.js", "const a = require(\"./a.js\");\nexports.sawEarly = String(a.early);\n"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 const a: any = require(\"./a.js\");\n\
                 test(\"b saw what a published first\", () =>\n\
                   expect(a.whatBSaw).toBe(\"published-before-require\"));\n",
            ),
        ],
    );
    let report = run(&dir.join("app.ts"));
    assert!(!report.contains("FAIL"), "{report}");
}

/// A module runs ONCE, however many times it is required.
///
/// The `running` flag makes a cycle terminate; this is the other half of the
/// same state — a module that already ran is not run again, so a side effect in
/// its body happens once.
#[test]
fn a_module_body_runs_once() {
    let dir = fixture(
        "once",
        &[
            ("counted.js", "globalThis.__ran = (globalThis.__ran || 0) + 1;\nexports.n = 1;\n"),
            ("mid.js", "require(\"./counted.js\");\nexports.ok = true;\n"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 require(\"./counted.js\");\n\
                 require(\"./mid.js\");\n\
                 require(\"./counted.js\");\n\
                 test(\"the body ran once\", () => expect((globalThis as any).__ran).toBe(1));\n",
            ),
        ],
    );
    let report = run(&dir.join("app.ts"));
    assert!(!report.contains("FAIL"), "{report}");
}

/// A STATIC import of a cycle links too, not only `require`.
///
/// `module_binding` is an entry point at run time and is one of the four places
/// a module gets named — the others being `module_namespace`, `module_import`
/// and `require`. All four had to learn to run the body; this is the one that is
/// easy to forget, because an `import` looks resolved at compile time.
#[test]
fn a_static_import_cycle_links() {
    let dir = fixture(
        "static",
        &[
            ("a.ts", "import { b } from \"./b\";\nexport const a = 1;\nexport const fromB = b;\n"),
            ("b.ts", "import { a } from \"./a\";\nexport const b = 2;\nexport const fromA = a;\n"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 import { a, fromB } from \"./a\";\n\
                 test(\"a's own export\", () => expect(a).toBe(1));\n\
                 test(\"and what it read of b\", () => expect(fromB).toBe(2));\n",
            ),
        ],
    );
    let report = run(&dir.join("app.ts"));
    assert!(!report.contains("FAIL"), "{report}");
}

/// A module nothing names does NOT run, which is the behaviour change this
/// carries and the one worth stating.
///
/// Every body used to run, so a side effect in a module reached by no import and
/// no `require` happened anyway. It no longer does. That is Node's behaviour and
/// it is a change a program could depend on, so it is pinned rather than left to
/// be discovered.
#[test]
fn a_module_nothing_names_does_not_run() {
    let dir = fixture(
        "unreached",
        &[
            ("used.js", "exports.n = 1;\n"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 const used: any = require(\"./used.js\");\n\
                 test(\"the one it asked for ran\", () => expect(used.n).toBe(1));\n",
            ),
        ],
    );
    let report = run(&dir.join("app.ts"));
    assert!(!report.contains("FAIL"), "{report}");
}
