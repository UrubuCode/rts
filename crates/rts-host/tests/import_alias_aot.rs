//! An aliased module is IN the object, like any other module of the graph.
//!
//! # What this does not claim
//!
//! That the two destinations answer the same thing. Running an object needs a
//! linker and `rts-runtime`'s staticlib, and `cargo test` builds neither —
//! `aot_object.rs`'s header is the long form. That claim is the blocking
//! smoke's, and `tests/aot/graph.ts` now imports through an alias so the
//! smoke makes it.

use object::{Object, ObjectSection, ObjectSymbol};
use rts_host::object::{MODULE_TABLE_SYMBOL, compile_graph_to_object};
use std::path::PathBuf;

fn graph(named: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join("rts-alias-aot").join(named);
    let _ = std::fs::remove_dir_all(&dir);
    let mut entry = PathBuf::new();
    for (name, source) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(&path, source).expect("a file to write");
        if name.ends_with("app.ts") {
            entry = path;
        }
    }
    entry
}

/// One entry of an address table, in bytes. Copied from `aot_object.rs`
/// rather than shared — the crate keeps no `common.rs`/`mod.rs` among its
/// test binaries (`ls crates/rts-host/tests/` before writing this file
/// confirmed it), so each integration test file is its own crate and cannot
/// import another one's private helpers.
fn width() -> u64 {
    size_of::<*const u8>() as u64
}

/// How many relocations the object carries inside the table named `symbol`.
///
/// Copied verbatim from `aot_object.rs:52-73`.
fn entries_in(bytes: &[u8], symbol: &str, expected: usize) -> usize {
    let file = object::File::parse(bytes).expect("a well-formed object file");
    let table = file
        .symbols()
        .find(|found| found.name() == Ok(symbol))
        .unwrap_or_else(|| panic!("`{symbol}` is not in the object — the archive would not link"));
    assert!(!table.is_undefined(), "`{symbol}` is defined by this object");
    let first = table.address() + width();
    let span = first..first + expected as u64 * width();
    let section = file
        .section_by_index(table.section_index().expect("the table is in a section"))
        .expect("the section it named exists");
    section
        .relocations()
        .filter(|(at, _)| span.contains(at))
        .count()
}

/// A static alias is rewritten by `rewrite` before either destination sees it,
/// so the object must carry the aliased module as an ordinary one. Spec §6.
#[test]
fn the_object_carries_a_module_reached_through_an_alias() {
    let entry = graph(
        "static",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/lib/adder.ts", "export function add(a: number, b: number) { return a + b; }\n"),
            (
                "src/app.ts",
                "import { add } from \"@/lib/adder\";\nconsole.log(add(2, 3));\n",
            ),
        ],
    );
    let program = compile_graph_to_object(&entry).expect("an aliased graph compiles to an object");
    // Two modules: the entry and the one it reached through the alias. If the
    // alias had been left unresolved the graph would be one module, and the
    // import would have become an undefined name at link time rather than a
    // failure here.
    assert_eq!(
        program.modules, 1,
        "one module runs before the entry: the file the alias reached"
    );
    let count = entries_in(&program.bytes, MODULE_TABLE_SYMBOL, 1);
    assert_eq!(count, 1, "the aliased module is in the object's module table");
}

// # The computed specifier — Step 6 asked for a distinction that does not
// # exist, verified rather than assumed
//
// Step 6 of the brief asks for a test that `require("@/" + name)` is refused
// BY NAME under AOT while resolving under JIT. Investigating turned up that
// this premise is wrong, not merely untestable here:
//
// `crates/rts-runtime-boot/src/resolver.rs`'s own doc (resolver.rs:47-52)
// says what the AOT resolver does with a specifier that is not a literal the
// manifest recorded: "`None` for anything not in it — a bare name, a `node:`
// specifier, or a computed one — which leaves the specifier as the program
// wrote it." So on the AOT side a computed specifier is never looked up by
// name at all; there is no refusal distinct from the ordinary
// `rts_core::entry::common_js.rs:110` message
// (`"cannot find module \"{wanted}\" — nothing registered that specifier"`).
//
// The test below shows the SAME thing is true on the JIT side, and for the
// same underlying reason: `rts_host::compile_graph`'s walk
// (`crates/rts-host/src/graph/walk.rs`) statically discovers every specifier
// it can register ahead of running — literal `import`s and literal
// `require("...")` calls both, which is why `import_alias.rs`'s
// `a_literal_dynamic_import_of_an_alias_resolves_at_run_time` (a *literal*
// string handed to `require` at run time) already passes: the walker saw
// that literal text and registered the module before the program ran.
// `require("@/" + name)` is opaque to that walk — nothing rewrites or
// pre-registers a concatenation — and `rts_core::entry::module_import`'s own
// doc (quoted in this crate's README, "What it does not do yet") is explicit
// that a module is read from an ALREADY-REGISTERED table, never loaded on
// demand. So under JIT the runtime resolver still translates the computed
// text to a canonical path (`context.resolver` answers something), but
// `value_of` finds no module registered under it, and `require_call` raises
// the exact same "cannot find module" message AOT would.
//
// In short: neither destination distinguishes "computed" from "any specifier
// nothing registered", and neither resolves a genuinely computed specifier
// that the static walk could not see. The distinction Step 6 asked to prove
// does not exist to prove, so what is asserted below is the behaviour this
// crate actually has — refusal, by the existing message, under JIT — which is
// the half a crate test can reach at all.

/// A specifier built at run time (`"@/" + "lib/late"`) is invisible to the
/// static walk that registers every module `require` can find ahead of
/// running, on EITHER destination — see the comment above. So `require` of it
/// raises `crate::common_js`'s ordinary "nothing registered" message, naming
/// the exact text the program handed it, rather than silently resolving.
#[test]
fn a_computed_specifier_is_refused_by_name_under_jit_too() {
    let dir = std::env::temp_dir().join("rts-alias-aot").join("computed");
    let _ = std::fs::remove_dir_all(&dir);
    let files: &[(&str, &str)] = &[
        ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
        ("src/lib/late.ts", "export const value = 7;\n"),
        (
            "src/app.ts",
            "import { test, expect } from \"rts:test\";\n\
             const name = \"lib/late\";\n\
             test(\"computed\", () => {\n\
             \x20 try {\n\
             \x20   require(\"@/\" + name);\n\
             \x20   expect(\"resolved\").toBe(\"refused\");\n\
             \x20 } catch (error) {\n\
             \x20   expect(String(error.message)).toBe(\n\
             \x20     'cannot find module \"@/lib/late\" — nothing registered that specifier'\n\
             \x20   );\n\
             \x20 }\n\
             });\n",
        ),
    ];
    for (name, source) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(&path, source).expect("a file to write");
    }

    rts_std::test::reset();
    let mut program = rts_host::compile_graph(&dir.join("src/app.ts")).expect("the graph compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    assert_eq!(reported.len(), 1);
    assert!(
        failed.is_empty(),
        "the refusal names the specifier the program wrote, as `common_js.rs`'s own \
         message does: {failed:?}"
    );
}
