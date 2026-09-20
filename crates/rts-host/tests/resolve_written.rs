//! "Does this name a file, and which" — asked once, answered once.

use rts_host::graph::{names_the_host, resolve_written, Aliases};
use std::io::Write;

fn fixture(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rts_written_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, source) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
    }
    dir
}

/// Spec §4 row 2, and the reason it is not negotiable: a project whose
/// `baseUrl` contains a directory named `node` must not shadow `node:fs`.
#[test]
fn a_scheme_is_never_a_file_even_when_a_file_would_match() {
    let dir = fixture(
        "scheme",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\".\"}}"),
            ("node/fs.ts", "export const x = 1;\n"),
            ("rts/egui.ts", "export const x = 1;\n"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    assert!(names_the_host("node:fs"));
    assert!(names_the_host("rts:egui"));
    assert_eq!(resolve_written(&entry, "node:fs", &aliases), None, "the file must not win");
    assert_eq!(resolve_written(&entry, "rts:egui", &aliases), None);
}

/// A Windows absolute specifier has a scheme's shape and is left as written,
/// which is what happens today — spec §4.
#[test]
fn a_windows_absolute_specifier_is_left_as_written() {
    assert!(names_the_host("C:/somewhere/x"));
}

/// Spec §4 row 4: `baseUrl` missing does not fail, it falls through, or every
/// package import in a project with `baseUrl` would break.
#[test]
fn a_bare_name_base_url_cannot_find_falls_through() {
    let dir = fixture(
        "fallthrough",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\".\"}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    assert_eq!(resolve_written(&entry, "lodash", &aliases), None, "not a file, so the host's");
}

/// The relative path still works, and is still the first question asked.
#[test]
fn a_relative_specifier_resolves_as_it_always_did() {
    let dir = fixture(
        "relative",
        &[("src/app.ts", "export const x = 1;\n"), ("src/other.ts", "export const y = 2;\n")],
    );
    let entry = dir.join("src/app.ts");
    let found = resolve_written(&entry, "./other", &Aliases::none()).expect("resolved");
    assert_eq!(found, dir.join("src").join("other.ts"));
}

/// The alias resolves, and it goes through `extended` — the `.ts` was not
/// written and the caller did not add it.
#[test]
fn an_alias_resolves_through_the_existing_extension_rule() {
    let dir = fixture(
        "alias",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
            ("src/engine/scene.ts", "export const y = 2;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/engine/scene", &aliases).expect("resolved");
    assert_eq!(found, dir.join("src").join("engine").join("scene.ts"));
}

/// An alias naming a directory takes its `index.*`, because `extended` does.
#[test]
fn an_alias_naming_a_directory_takes_its_index() {
    let dir = fixture(
        "aliasdir",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
            ("src/engine/index.ts", "export const y = 2;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/engine", &aliases).expect("resolved");
    assert_eq!(found, dir.join("src").join("engine").join("index.ts"));
}

/// Spec §9 point 4: the list is tried in order and the first that EXISTS wins
/// — not the first that is written.
#[test]
fn a_list_of_targets_falls_through_to_the_one_that_exists() {
    let dir = fixture(
        "list",
        &[
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./missing/*\",\"./real/*\"]}}}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
            ("real/thing.ts", "export const y = 2;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/thing", &aliases).expect("resolved");
    assert_eq!(found, dir.join("real").join("thing.ts"));
}

/// Spec §9 point 3: longest literal prefix, NOT source order. Written with the
/// general pattern FIRST so source order would give the wrong answer.
#[test]
fn the_longest_prefix_wins_over_source_order() {
    let dir = fixture(
        "longest",
        &[
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./general/*\"],\"@/engine/*\":[\"./specific/*\"]}}}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
            ("general/engine/scene.ts", "export const y = 2;\n"),
            ("specific/scene.ts", "export const z = 3;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/engine/scene", &aliases).expect("resolved");
    assert_eq!(found, dir.join("specific").join("scene.ts"), "@/engine/* is more specific");
}

/// An exact key beats a wildcard that also matches — spec §9 point 2.
#[test]
fn an_exact_key_beats_a_wildcard() {
    let dir = fixture(
        "exact",
        &[
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./general/*\"],\"@/one\":[\"./exact/one.ts\"]}}}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
            ("general/one.ts", "export const y = 2;\n"),
            ("exact/one.ts", "export const z = 3;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/one", &aliases).expect("resolved");
    assert_eq!(found, dir.join("exact").join("one.ts"));
}

/// Spec §9 point 4: the list is tried IN ORDER, to the end. A target that has
/// no file name — one ending in `..`, which `tsc` accepts and this engine
/// does not refuse — used to answer `None` from the whole function, throwing
/// away every later target and the `baseUrl` fall-through with it.
#[test]
fn a_target_with_no_file_name_does_not_abort_the_list() {
    let dir = fixture(
        "malformed",
        &[
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"baseUrl\":\".\",\
                 \"paths\":{\"@/*\":[\"./nowhere/..\",\"./real/*\"]}}}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
            ("real/thing.ts", "export const y = 2;\n"),
        ],
    );
    let entry = dir.join("src/app.ts");
    let aliases = Aliases::discover(&entry);
    let found = resolve_written(&entry, "@/thing", &aliases).expect("the later target still wins");
    assert_eq!(found, dir.join("real").join("thing.ts"));
}
