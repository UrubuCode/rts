//! What a `tsconfig.json` says, read the way the loader will read it.

use rts_host::graph::Aliases;
use std::io::Write;

fn fixture(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rts_tsconfig_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, source) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        let mut file = std::fs::File::create(&path).expect("a fixture file");
        file.write_all(source.as_bytes()).expect("written");
    }
    dir
}

/// The guarantee the whole change rests on.
#[test]
fn no_tsconfig_anywhere_is_an_empty_map() {
    let dir = fixture("none", &[("src/app.ts", "export const x = 1;\n")]);
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert!(aliases.is_empty(), "no config found means no map, and today's behaviour");
}

#[test]
fn a_config_beside_the_entry_is_found() {
    let dir = fixture(
        "beside",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert!(!aliases.is_empty());
    let found = aliases.candidates("@/engine/scene");
    assert_eq!(found, vec![dir.join("src").join("engine").join("scene")]);
}

/// Upward, like node and bun and tsc: the entry is deep and the config is not.
#[test]
fn a_config_above_the_entry_is_found() {
    let dir = fixture(
        "above",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/deep/deeper/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/deep/deeper/app.ts"));
    assert_eq!(aliases.candidates("@/a"), vec![dir.join("src").join("a")]);
}

/// §9 point 5 of the spec — the one most likely to be got wrong. A target is
/// relative to the file that WROTE it, which `extends` makes visible.
#[test]
fn extends_resolves_targets_against_the_file_that_wrote_them() {
    let dir = fixture(
        "extends",
        &[
            ("base/tsconfig.base.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./lib/*\"]}}}"),
            ("tsconfig.json", "{\"extends\":\"./base/tsconfig.base.json\"}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert_eq!(
        aliases.candidates("@/thing"),
        vec![dir.join("base").join("lib").join("thing")],
        "the target was written in base/, so ./lib/ is base/lib/ and not the root's"
    );
}

/// A child's own `paths` wins over what it extends, key by key.
#[test]
fn a_child_overrides_the_key_it_redefines_and_keeps_the_rest() {
    let dir = fixture(
        "override",
        &[
            ("tsconfig.base.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./old/*\"],\"~/*\":[\"./keep/*\"]}}}"),
            ("tsconfig.json", "{\"extends\":\"./tsconfig.base.json\",\"compilerOptions\":{\"paths\":{\"@/*\":[\"./new/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert_eq!(aliases.candidates("@/a"), vec![dir.join("new").join("a")]);
    assert_eq!(aliases.candidates("~/a"), vec![dir.join("keep").join("a")]);
}

/// JSONC, because `tsc --init` writes it that way.
#[test]
fn comments_and_a_trailing_comma_do_not_stop_it() {
    let dir = fixture(
        "jsonc",
        &[
            (
                "tsconfig.json",
                "{\n // the project\n \"compilerOptions\": {\n /* aliases */\n \"paths\": {\"@/*\": [\"./src/*\"],},\n },\n}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert_eq!(aliases.candidates("@/a"), vec![dir.join("src").join("a")]);
}

/// A specifier nothing matches asks the disk for nothing.
#[test]
fn a_specifier_no_pattern_matches_has_no_candidates() {
    let dir = fixture(
        "nomatch",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert!(aliases.candidates("lodash").is_empty());
}

/// `baseUrl` alone, with no `paths`, makes every bare name a candidate.
#[test]
fn base_url_alone_makes_a_bare_name_a_candidate() {
    let dir = fixture(
        "baseurl",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\"./src\"}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert_eq!(aliases.candidates("engine/scene"), vec![dir.join("src").join("engine").join("scene")]);
}
