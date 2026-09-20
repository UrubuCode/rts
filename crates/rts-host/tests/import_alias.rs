//! An alias, exercised the way a program reaches one: by running.
//!
//! Rule 5 of this crate — a test here runs the program. The assertions are
//! made from INSIDE, through `rts:test`, because a module answers nothing to
//! its host.

use std::io::Write;

fn fixture(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rts_alias_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, source) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
    }
    dir
}

/// Runs the entry and answers the failures its `rts:test` calls recorded.
fn run(entry: &std::path::Path) -> (usize, Vec<String>) {
    rts_std::test::reset();
    let mut program = rts_host::compile_graph(entry).expect("the graph compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    (reported.len(), failed)
}

#[test]
fn a_static_alias_import_runs_and_answers() {
    let dir = fixture(
        "static",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/lib/adder.ts", "export function add(a: number, b: number) { return a + b; }\n"),
            (
                "src/app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 import { add } from \"@/lib/adder\";\n\
                 test(\"aliased\", () => expect(add(2, 3)).toBe(5));\n",
            ),
        ],
    );
    let (count, failed) = run(&dir.join("src/app.ts"));
    assert_eq!(count, 1, "the fixture registers one test");
    assert!(failed.is_empty(), "the alias resolved and the module ran: {failed:?}");
}

/// The no-config guarantee, as a running program: the same shape with no
/// `tsconfig.json` still resolves its relative imports and still runs.
#[test]
fn a_program_with_no_tsconfig_is_unaffected() {
    let dir = fixture(
        "noconfig",
        &[
            ("src/lib/adder.ts", "export function add(a: number, b: number) { return a + b; }\n"),
            (
                "src/app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 import { add } from \"./lib/adder\";\n\
                 test(\"relative\", () => expect(add(2, 3)).toBe(5));\n",
            ),
        ],
    );
    let (count, failed) = run(&dir.join("src/app.ts"));
    assert_eq!(count, 1);
    assert!(failed.is_empty(), "no config must change nothing: {failed:?}");
}

/// Spec §4 row 2, proven by running: the host module wins over a file that
/// `baseUrl` would otherwise find.
#[test]
fn a_host_module_still_wins_with_base_url_set() {
    let dir = fixture(
        "hostwins",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\".\"}}"),
            // A file that `baseUrl` would resolve `rts:test` to, if a scheme
            // were ever treated as a path. It exports something WRONG on
            // purpose, so reaching it fails loudly instead of passing.
            ("rts/test.ts", "export const test = 0;\nexport const expect = 0;\n"),
            (
                "src/app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 test(\"the harness, not the file\", () => expect(1).toBe(1));\n",
            ),
        ],
    );
    let (count, failed) = run(&dir.join("src/app.ts"));
    assert_eq!(count, 1, "reaching rts/test.ts instead would register nothing");
    assert!(failed.is_empty(), "{failed:?}");
}

/// The runtime hook, not the loader's walk. This is the test that catches the
/// `thread_local!` risk: a dynamic import resolves through
/// `declare_resolver`, which runs while the PROGRAM runs.
#[test]
fn a_literal_dynamic_import_of_an_alias_resolves_at_run_time() {
    let dir = fixture(
        "dynamic",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/lib/late.ts", "export const value = 7;\n"),
            (
                "src/app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 const mod = require(\"@/lib/late\");\n\
                 test(\"late\", () => expect(mod.value).toBe(7));\n",
            ),
        ],
    );
    let (count, failed) = run(&dir.join("src/app.ts"));
    assert_eq!(count, 1);
    assert!(failed.is_empty(), "the runtime resolver saw the map: {failed:?}");
}

/// A cycle through an alias is refused by name, as a relative cycle is.
#[test]
fn a_cycle_through_an_alias_is_refused_by_name() {
    let dir = fixture(
        "cycle",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/a.ts", "import { b } from \"@/b\";\nexport const a = b;\n"),
            ("src/b.ts", "import { a } from \"@/a\";\nexport const b = a;\n"),
        ],
    );
    let error = rts_host::compile_graph(&dir.join("src/a.ts")).expect_err("a cycle is refused");
    let text = format!("{error:?}");
    assert!(
        text.to_lowercase().contains("cycle"),
        "the refusal names the cycle rather than failing obscurely: {text}"
    );
}

/// ONE file is ONE module, whichever of its two written names reached it.
///
/// The risk this settles, and it is only observable by RUNNING: the relative
/// branch of `resolve_written` answers through `resolve`, whose tail
/// canonicalises before `plain`; the alias branch applies `plain` to a path it
/// merely joined. Canonicalising resolves symlinks and collapses `..` and not
/// canonicalising does neither, so two written names for one file could come
/// back as two different strings — and the loader keys a module by that
/// string. Two keys are two modules with two namespaces, which is the exact
/// failure spec §5 ("one map per program") exists to prevent.
///
/// A counter is the cheapest observation of it: state is per MODULE, so if the
/// two names are two modules, the bump made through one is invisible through
/// the other, and the value read back is the initial one.
#[test]
fn two_written_names_for_one_file_are_one_module() {
    let dir = fixture(
        "onemodule",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            (
                "src/lib/counter.ts",
                "export let count = 0;\n\
                 export function bump() { count = count + 1; return count; }\n\
                 export function read() { return count; }\n",
            ),
            (
                "src/app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 import { bump } from \"./lib/counter\";\n\
                 import { read } from \"@/lib/counter\";\n\
                 bump();\n\
                 bump();\n\
                 test(\"one file, one module\", () => expect(read()).toBe(2));\n",
            ),
        ],
    );
    let (count, failed) = run(&dir.join("src/app.ts"));
    assert_eq!(count, 1);
    assert!(
        failed.is_empty(),
        "the relative name and the aliased name reached the SAME module; a 0 here \
         means two modules with two namespaces for one file: {failed:?}"
    );
}

/// The same claim, with the one target shape that makes the two spellings
/// genuinely differ on disk: a `paths` target that walks OUT and back in.
///
/// Spec §7 allows a target to escape the project (`"@lib/*":
/// ["../../shared/*"]`), so a `..` in a resolved alias path is a supported
/// case and not a contrivance. A joined path keeps its `..`; a canonicalised
/// one collapses it. If the alias branch did not canonicalise, this file
/// would arrive under two keys and `read()` would answer 0.
#[test]
fn a_paths_target_that_walks_out_and_back_is_still_one_module() {
    let dir = fixture(
        "escaping",
        &[
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/lib/../lib/*\"]}}}",
            ),
            (
                "src/lib/counter.ts",
                "export let count = 0;
                 export function bump() { count = count + 1; return count; }
                 export function read() { return count; }
",
            ),
            (
                "src/app.ts",
                "import { test, expect } from \"rts:test\";
                 import { bump } from \"./lib/counter\";
                 import { read } from \"@/counter\";
                 bump();
                 test(\"one file, one module, through a ..\", () => expect(read()).toBe(1));
",
            ),
        ],
    );
    let (count, failed) = run(&dir.join("src/app.ts"));
    assert_eq!(count, 1);
    assert!(failed.is_empty(), "a `..` in the target must not make a second module: {failed:?}");
}

/// A remote program must not resolve `@/…` against THIS machine's project.
///
/// The mirror (`rts run https://…`, `rts-cli/src/url_entry.rs`) asks
/// `relative_imports`, which is blind to aliases. The fixture installs a map
/// that WOULD match, so a pass means the blindness held rather than that
/// nothing matched.
#[test]
fn the_remote_mirror_does_not_resolve_an_alias() {
    let dir = fixture(
        "remote",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/secret.ts", "export const value = 1;\n"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    // Make the local map ACTIVE, exactly as loading a local program would.
    let entry = dir.join("src/app.ts");
    rts_host::compile_graph(&entry).expect("the local program compiles, installing the map");

    let remote = "import { value } from \"@/secret\";\nexport const y = value;\n";
    let found = rts_host::graph::relative_imports(remote).expect("it parses");
    assert!(
        found.is_empty(),
        "a remote program's `@/secret` must not become this machine's file: {found:?}"
    );

    // And the control: a relative one IS still fetched, so the test above is
    // not passing because the walk answers nothing at all.
    let relative = "import { value } from \"./secret\";\nexport const y = value;\n";
    let found = rts_host::graph::relative_imports(relative).expect("it parses");
    assert_eq!(found, vec!["./secret".to_string()]);
}

/// The defect task 9 fixes, at the level where it was decided.
///
/// A program whose ONLY file-naming import is an ALIAS. Before the fix, the
/// caller that chooses between a graph compile and a single-file compile asked
/// a substring test for `./`/`../`, so this source answered "names no file",
/// was compiled alone, and died at run time on
/// `cannot resolve module "@/..." — nothing registered that specifier`.
///
/// `rts_host::names_any_file` is the one answer, and it must say yes here.
#[test]
fn an_alias_only_program_names_a_file() {
    let dir = fixture(
        "aliasonly_decide",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/compat/io.ts", "export const value = 7;\n"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 import { value } from \"@/compat/io.ts\";\n\
                 test(\"aliased\", () => expect(value).toBe(7));\n",
            ),
        ],
    );
    let entry = dir.join("app.ts");
    let source = std::fs::read_to_string(&entry).expect("the fixture reads");
    assert!(
        rts_host::names_any_file(&source, &entry).expect("it parses"),
        "an alias names a file exactly as `./` does"
    );

    // And the control: the same program with the alias removed names nothing,
    // so the assertion above is not passing because the answer is always yes.
    let alone = "import { test, expect } from \"rts:test\";\ntest(\"x\", () => expect(1).toBe(1));\n";
    assert!(
        !rts_host::names_any_file(alone, &entry).expect("it parses"),
        "`rts:test` is answered by the runtime and names no file"
    );
}

/// The same program, RUN — rule 5. The decision above is taken the way
/// `rts run` takes it, so this fails end to end if the decision regresses.
#[test]
fn an_alias_only_program_runs() {
    let dir = fixture(
        "aliasonly_run",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/compat/io.ts", "export const value = 7;\n"),
            (
                "app.ts",
                "import { test, expect } from \"rts:test\";\n\
                 import { value } from \"@/compat/io.ts\";\n\
                 test(\"aliased\", () => expect(value).toBe(7));\n",
            ),
        ],
    );
    let entry = dir.join("app.ts");
    let source = std::fs::read_to_string(&entry).expect("the fixture reads");

    rts_std::test::reset();
    let compiled = match rts_host::names_any_file(&source, &entry).expect("it parses") {
        true => rts_host::compile_graph(&entry),
        false => rts_host::compile(&source),
    };
    let mut program = compiled.expect("the program compiles");
    program.run();
    let reported = rts_std::test::record();
    let failed: Vec<String> = reported.iter().filter_map(|one| one.failure.clone()).collect();
    assert_eq!(reported.len(), 1, "the fixture registers one test");
    assert!(failed.is_empty(), "the alias resolved with no relative import beside it: {failed:?}");
}
