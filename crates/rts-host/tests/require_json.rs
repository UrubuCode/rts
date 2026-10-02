//! `require("./x.json")` answers the parsed value — #2853.
//!
//! The loader read every file a specifier reached as JavaScript, `.json`
//! included, so `sharp`'s `require("./package.json")` — the ordinary way a
//! package reads its own version — failed with
//! `Syntax("Expected ';', '}' or <eof>")`. That is what stopped
//! `@whiskeysockets/baileys` after the resolution and ordering fixes.
//!
//! # Why the body is `JSON.parse` of a string literal rather than the text inline
//!
//! Because JSON is not a subset of JavaScript EXPRESSIONS in the ways that
//! matter here. `{"__proto__": 1}` is an own property in JSON and sets the
//! prototype in an object literal; U+2028 and U+2029 are legal raw in a JSON
//! string and were line terminators in JS until ES2019. Handing the text to
//! `JSON.parse` uses the one parser that already has those rules right, instead
//! of a second one that has to agree with it.

use std::io::Write;
use std::path::PathBuf;

fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rts_require_json_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, source) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
    }
    dir
}

/// Runs the entry as a graph and answers what it printed.
fn run(entry: &PathBuf) -> String {
    let source = std::fs::read_to_string(entry).expect("the fixture reads");
    assert!(
        rts_host::names_any_file(&source, entry).expect("it parses"),
        "the fixture must name a file, or this tests the single-file path"
    );
    rts_std::test::reset();
    let mut program = rts_host::compile_graph(entry).expect("the program compiles");
    program.run();
    let reported = rts_std::test::record();
    reported
        .iter()
        .map(|one| match &one.failure {
            Some(why) => format!("FAIL {}: {why}", one.name),
            None => format!("ok {}", one.name),
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The `sharp` shape: a package reading its own version.
#[test]
fn a_json_file_required_answers_its_value() {
    let dir = fixture(
        "value",
        &[
            ("thing.json", "{ \"name\": \"thing\", \"version\": \"1.2.3\", \"n\": [1, 2, 3] }"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 const pkg: any = require(\"./thing.json\");\n\
                 test(\"name\", () => expect(pkg.name).toBe(\"thing\"));\n\
                 test(\"version\", () => expect(pkg.version).toBe(\"1.2.3\"));\n\
                 test(\"array\", () => expect(pkg.n[2]).toBe(3));\n\
                 test(\"it is an object\", () => expect(typeof pkg).toBe(\"object\"));\n",
            ),
        ],
    );
    let report = run(&dir.join("app.ts"));
    assert!(!report.contains("FAIL"), "{report}");
    assert_eq!(report.matches("ok ").count(), 4, "{report}");
}

/// `import x from "./x.json"` is the default export — the same value.
#[test]
fn a_json_file_imported_as_default_is_the_same_value() {
    let dir = fixture(
        "default",
        &[
            ("data.json", "{ \"a\": 1 }"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 import data from \"./data.json\";\n\
                 test(\"default\", () => expect((data as any).a).toBe(1));\n",
            ),
        ],
    );
    let report = run(&dir.join("app.ts"));
    assert!(!report.contains("FAIL"), "{report}");
}

/// JSON's own semantics, not an object literal's: `__proto__` is an OWN
/// property. This is the case that makes the body `JSON.parse` of a literal
/// rather than the text inlined as an expression — inlined, this sets the
/// prototype and `hasOwnProperty` answers false.
#[test]
fn json_semantics_and_not_an_object_literals() {
    let dir = fixture(
        "proto",
        &[
            ("odd.json", "{ \"__proto__\": { \"tag\": 1 }, \"kept\": 2 }"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 const odd: any = require(\"./odd.json\");\n\
                 test(\"__proto__ is an own property\", () =>\n\
                   expect(Object.prototype.hasOwnProperty.call(odd, \"__proto__\")).toBe(true));\n\
                 test(\"and the sibling survived\", () => expect(odd.kept).toBe(2));\n",
            ),
        ],
    );
    let report = run(&dir.join("app.ts"));
    assert!(!report.contains("FAIL"), "{report}");
}

/// A quote and a backslash in the text survive the trip, which is what says the
/// content is escaped rather than concatenated.
#[test]
fn quotes_and_backslashes_survive() {
    let dir = fixture(
        "escapes",
        &[
            ("text.json", "{ \"quote\": \"he said \\\"hi\\\"\", \"path\": \"C:\\\\x\\\\y\" }"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 const t: any = require(\"./text.json\");\n\
                 test(\"quote\", () => expect(t.quote).toBe('he said \"hi\"'));\n\
                 test(\"backslash\", () => expect(t.path.length).toBe(6));\n",
            ),
        ],
    );
    let report = run(&dir.join("app.ts"));
    assert!(!report.contains("FAIL"), "{report}");
}

/// A malformed `.json` is refused at LOAD time, naming the file and the JSON
/// fault — not "Expected ';', '}' or <eof>", which describes a JavaScript parse
/// of a file nobody meant as JavaScript.
#[test]
fn a_malformed_json_names_the_json_fault() {
    let dir = fixture(
        "malformed",
        &[
            ("bad.json", "{ \"a\": }"),
            ("app.ts", "const bad: any = require(\"./bad.json\");\nexport const x = bad;\n"),
        ],
    );
    let entry = dir.join("app.ts");
    let message = match rts_host::graph::load(&entry) {
        Ok(_) => panic!("a malformed .json is not a module"),
        Err(error) => format!("{error:#?}"),
    };
    assert!(message.contains("bad.json"), "the file belongs in the message: {message}");
    assert!(
        message.contains("is not valid JSON"),
        "and the fault must be named as a JSON one: {message}"
    );
}

/// `./thing` with only `thing.json` beside it resolves, which is Node's
/// `LOAD_AS_FILE` trying `X.json` after `X.js`.
#[test]
fn an_extensionless_specifier_finds_a_json_file() {
    let dir = fixture(
        "extensionless",
        &[
            ("only.json", "{ \"found\": true }"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 const o: any = require(\"./only\");\n\
                 test(\"found\", () => expect(o.found).toBe(true));\n",
            ),
        ],
    );
    let report = run(&dir.join("app.ts"));
    assert!(!report.contains("FAIL"), "{report}");
}
