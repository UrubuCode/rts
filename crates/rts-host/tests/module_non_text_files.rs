//! What a specifier reaches that is NOT a JavaScript module — #2859, and the
//! `.node` half of what stopped `@whiskeysockets/baileys`.
//!
//! # The class, and why the MESSAGE is the test
//!
//! Three times in this session the loader read something that is not text as
//! text, and each time the error described the wrong fault:
//!
//! | reached | what it said | what it was |
//! |---|---|---|
//! | a directory | `Acesso negado. (os error 5)` | an ordering bug (#2854) |
//! | a `.json` | `Expected ';', '}' or <eof>` | data, not a program (#2853) |
//! | a `.node` | `stream did not contain valid UTF-8` | a native binary |
//!
//! None of them said "this is not a module of text", which was the fault in all
//! three. So these tests assert the message as much as the behaviour: a reader
//! sent to the wrong line is the cost being removed.

use std::io::Write;
use std::path::PathBuf;

fn fixture(name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rts_non_text_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, bytes) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(bytes).expect("written");
    }
    dir
}

/// A `.node` addon COMPILES, and its body raises when it runs.
///
/// Both halves are the point. The bytes below are not UTF-8, so reading them as
/// text is the `stream did not contain valid UTF-8` this removes — and that
/// failure killed the whole compilation, which is the wrong shape for the way a
/// native addon is actually asked for:
///
/// ```js
/// await Promise.all([import('jimp').catch(() => {}), import('sharp').catch(() => {})])
/// ```
///
/// That is `@whiskeysockets/baileys`, and it is an OPTIONAL dependency with a
/// `catch`. A loader that fails while reading denies that `catch` its purpose:
/// the program never starts. A module whose body raises lets the program handle
/// it, which is the difference between "this dependency is unavailable" and
/// "this program cannot be built".
#[test]
fn a_native_addon_compiles_and_raises_when_run() {
    let dir = fixture(
        "addon",
        &[
            // Not valid UTF-8: a lone 0xFF, which is what made the old read fail.
            ("native.node", &[0x4d, 0x5a, 0xff, 0xfe, 0x00, 0x01]),
            (
                "app.ts",
                b"import { test, expect } from \"rts:test\";\n\
                  let message = \"did not throw\";\n\
                  try { require(\"./native.node\"); } catch (e: any) { message = String(e.message); }\n\
                  test(\"the body raised\", () => expect(message.indexOf(\"did not throw\")).toBe(-1));\n\
                  test(\"and named the addon\", () => expect(message.indexOf(\"native addon\") >= 0).toBe(true));\n\
                  test(\"and said what loads one today\", () => expect(message.indexOf(\"rts napi\") >= 0).toBe(true));\n",
            ),
        ],
    );
    let entry = dir.join("app.ts");
    let source = std::fs::read_to_string(&entry).expect("the fixture reads");
    assert!(
        rts_host::names_any_file(&source, &entry).expect("it parses"),
        "the fixture must name a file"
    );
    rts_std::test::reset();
    let mut program = rts_host::compile_graph(&entry)
        .expect("a .node file must not break the COMPILATION — that is the defect");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    assert_eq!(reported.len(), 3, "the fixture registers three tests");
    assert!(failed.is_empty(), "{failed:?}");
}

/// An addon nothing asks for does not raise at all.
///
/// This is the half that only works because a module body runs when something
/// NAMES it (#2852). Under the old topological sweep this body would have run
/// always, so a `.node` anywhere in the graph killed every program that merely
/// had one — including a program that never touched it.
#[test]
fn an_addon_nothing_requires_is_harmless() {
    let dir = fixture(
        "unused_addon",
        &[
            ("unused.node", &[0xff, 0xfe, 0x00]),
            ("used.js", b"module.exports = { n: 1 };\n"),
            (
                "app.ts",
                b"import { test, expect } from \"rts:test\";\n\
                  // The addon is in the graph because this file names it in a\n\
                  // string the loader follows, and NEVER required.\n\
                  const keep = \"./unused.node\";\n\
                  const used: any = require(\"./used.js\");\n\
                  test(\"the program ran\", () => expect(used.n).toBe(1));\n\
                  test(\"and the addon was never needed\", () => expect(keep.length > 0).toBe(true));\n",
            ),
        ],
    );
    let entry = dir.join("app.ts");
    rts_std::test::reset();
    let mut program = rts_host::compile_graph(&entry).expect("it compiles");
    program.run();
    let failed: Vec<String> = rts_std::test::record()
        .iter()
        .filter_map(|one| one.failure.clone())
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
}

/// `global` resolves at the TOP of a module body, which is where every npm
/// package reads it.
///
/// `rts-node` installs it (`declare_global(context, "global", …)`) and the
/// compiler's `PROVIDED` list did not have it, so the name did not resolve and
/// `typeof global` answered `"undefined"` about an object that exists — exactly
/// what that list's own `fetch` comment describes.
///
/// It hid from two directions at once: inside a FUNCTION the name resolved, and
/// in an ENTRY file it resolved too. Only the top of a module body failed, and
/// `typeof global !== "undefined"` is half the environment detection there is —
/// `protobufjs` writes it, and with `global` invisible its fallback chain ended
/// at `|| this` and produced `undefined.dcodeIO`.
#[test]
fn global_resolves_at_the_top_of_a_module_body() {
    let dir = fixture(
        "global_at_top",
        &[
            (
                "lib.js",
                b"var atTop = typeof global;\n\
                  var isNodeLike = Boolean(typeof global !== \"undefined\" && global\n\
                      && global.process && global.process.versions && global.process.versions.node);\n\
                  exports.atTop = atTop;\n\
                  exports.isNodeLike = isNodeLike;\n\
                  exports.inFunction = function () { return typeof global; };\n",
            ),
            (
                "app.ts",
                b"import { test, expect } from \"rts:test\";\n\
                  const lib: any = require(\"./lib.js\");\n\
                  test(\"at the top of the body\", () => expect(lib.atTop).toBe(\"object\"));\n\
                  test(\"inside a function, as before\", () => expect(lib.inFunction()).toBe(\"object\"));\n\
                  test(\"so an environment check answers true\", () => expect(lib.isNodeLike).toBe(true));\n",
            ),
        ],
    );
    let entry = dir.join("app.ts");
    rts_std::test::reset();
    let mut program = rts_host::compile_graph(&entry).expect("it compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    assert_eq!(reported.len(), 3);
    assert!(failed.is_empty(), "{failed:?}");
}
