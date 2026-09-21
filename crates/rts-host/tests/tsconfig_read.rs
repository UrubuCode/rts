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
    assert_eq!(
        found,
        vec![
            dir.join("src").join("engine").join("scene"),
            dir.join("@/engine/scene"),
        ],
        "the paths hit comes first; the baseUrl candidate follows it and is \
         what a matched-but-missing pattern falls through to"
    );
}

/// Spec §9 point 4: a pattern that MATCHES but whose every target is
/// missing must still fall through to `baseUrl`. Nothing covered this,
/// and an earlier fix removed the fall-through without a test failing.
#[test]
fn a_matched_pattern_whose_targets_all_miss_still_offers_base_url() {
    let dir = fixture(
        "fallthrough_base",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"./gone/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    let found = aliases.candidates("@/thing");
    assert_eq!(found.len(), 2, "the missing paths target, then baseUrl: {found:?}");
    assert!(found[1].starts_with(&dir), "the second candidate is the baseUrl one");
}

/// One file must have ONE spelling. `./src/*` and `baseUrl: "."` both
/// grow a `.` component when joined, and a module keyed by its resolved
/// path would then exist twice — spec §5's two-copies-of-one-module.
#[test]
fn no_candidate_carries_a_current_dir_component() {
    use std::path::Component;
    let dir = fixture(
        "nodot",
        &[
            ("tsconfig.json", "{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"./src/*\"]}}}"),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    let found = aliases.candidates("@/engine/scene");
    assert_eq!(found.len(), 2, "the paths hit and the baseUrl fall-through");
    for candidate in &found {
        assert!(
            !candidate.components().any(|part| part == Component::CurDir),
            "a `.` component survived into {candidate:?}"
        );
    }
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

/// The other half of §9 point 5, which the test above cannot see: with a
/// `baseUrl` set, the BASE URL is what a relative target resolves against —
/// not the config's directory. Invisible while `baseUrl` is `"."`, because
/// the two bases are then the same directory.
#[test]
fn base_url_is_the_base_a_relative_target_resolves_against() {
    let dir = fixture(
        "baseurl_targets",
        &[
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"baseUrl\":\"./src\",\"paths\":{\"@/*\":[\"lib/*\"]}}}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert_eq!(
        aliases.candidates("@/thing"),
        vec![
            dir.join("src").join("lib").join("thing"),
            // Row 4's fall-through, also from the base URL.
            dir.join("src").join("@").join("thing"),
        ],
        "baseUrl is ./src, so lib/* is src/lib/* and not the project root's"
    );
}

/// `extends` and `baseUrl` together: the inherited base URL is what the
/// child's targets resolve against, and a base URL is itself relative to the
/// file that wrote IT.
#[test]
fn an_inherited_base_url_is_the_base_for_a_childs_targets() {
    let dir = fixture(
        "baseurl_extends",
        &[
            ("base/tsconfig.base.json", "{\"compilerOptions\":{\"baseUrl\":\"../src\"}}"),
            (
                "tsconfig.json",
                "{\"extends\":\"./base/tsconfig.base.json\",\
                 \"compilerOptions\":{\"paths\":{\"@/*\":[\"lib/*\"]}}}",
            ),
            ("src/app.ts", "export const x = 1;\n"),
        ],
    );
    let aliases = Aliases::discover(&dir.join("src/app.ts"));
    assert_eq!(
        aliases.candidates("@/thing")[0],
        // The `..` survives here and is collapsed by `resolve::settled` on the
        // way to a module key, the same as any other target that walks out.
        dir.join("base").join("..").join("src").join("lib").join("thing"),
        "the base URL came from base/, and ../src from there is the root's src"
    );
}
