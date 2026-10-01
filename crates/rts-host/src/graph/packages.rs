//! What a BARE specifier names: the `node_modules` walk, and a package's own
//! `package.json`.
//!
//! # Why this is here and not reused from `rts-node`
//!
//! `rts-node`'s `module::packages` walks `node_modules` too, and answers a
//! different question: `module.findPackageJSON` asks *which `package.json`
//! encloses this*, which is a path and never a module. Its own doc says so, and
//! says in the same breath that it does NOT read `"exports"` because "that
//! redirection needs the resolution algorithm this crate does not have". This
//! is that algorithm, and it belongs beside [`super::resolve`] for the reason
//! that file's header gives: the loader, the runtime and the object path must
//! get ONE answer to "which file is this", and there is exactly one function
//! they all reach.
//!
//! So the duplication is the walk's four lines, not the answer. Making
//! `findPackageJSON` call this instead would invert the dependency — the
//! runtime surface asking the loader for a path it can compute itself — and
//! would make a native's answer depend on a loader being installed.
//!
//! # What is implemented, and what is not
//!
//! Read: `"exports"` (a string, an array of alternatives, a condition map, a
//! subpath map, and a `*` pattern), `"main"`, and the `index.*` fall-back.
//!
//! NOT read, by name:
//!
//! - **`"imports"`** — a `#private` specifier. It is resolved against the
//!   IMPORTING package rather than an imported one, so it is a different walk
//!   (the nearest enclosing `package.json`, not a `node_modules` one) and
//!   nothing asks for it yet.
//! - **`"exports"`'s null target** (`"./x": null`, which forbids a subpath).
//!   Treated as no match, so the walk continues upward instead of refusing.
//!   That is more permissive than Node, and the permissive direction cannot
//!   turn a working program into a wrong answer.
//! - **`"type"`** — this engine emits every file of a program into one
//!   compilation and binds `require`/`module`/`exports` beside `import`/`export`
//!   in every module, so there is no per-file format to decide. CLAUDE.md's
//!   "CommonJS is not a second module system here" is the rule; reading `"type"`
//!   here would be reading a field to then ignore it.

use std::path::{Path, PathBuf};

use super::resolve::{extended, settled};

/// The conditions this resolver matches, in the order it tries them.
///
/// # Why a fixed order rather than the object's own
///
/// Node tries the keys in the order the `"exports"` object WRITES them, and
/// `serde_json`'s default map is a `BTreeMap` — so the order this code could
/// read is alphabetical, which is nobody's intent. The alternative was
/// `serde_json/preserve_order`, and a feature is unified per package across a
/// Cargo build: turning it on here changes the key order every other crate in
/// this workspace SERIALISES with, to fix a read in one function. So the order
/// is written down instead, which is also testable where "the object's order"
/// is not.
///
/// `"rts"` leads because that is the condition a package publishes FOR this
/// engine — #2625 asked for it by name, and a library shipping TS to bun/rts and
/// bundled `.js` to node has no other way to say which is which. `"node"` is
/// next because this engine provides `node:*`, so a package splitting node from
/// a browser build means the node half here.
///
/// `"import"` before `"require"` although both are true here: a program's files
/// all enter ONE compilation and both module systems are bound in every module
/// (CLAUDE.md), so neither spelling is refused and the ESM entry is the one more
/// likely to be the package's source of truth. `"types"` is deliberately absent
/// — it names a declaration file, which is `tsc`'s answer and not a module.
const CONDITIONS: &[&str] = &["rts", "node", "import", "module", "require", "default"];

/// The file a bare specifier names, found through `node_modules`.
///
/// `None` means no package answered, which keeps [`super::resolve_written`]'s
/// established meaning: the specifier is left as the program wrote it and the
/// host provides it by name.
pub(super) fn resolve_bare(from: &Path, specifier: &str) -> Option<PathBuf> {
    let (package, subpath) = split(specifier)?;
    let mut directory = Some(from.parent().unwrap_or(Path::new(".")));
    while let Some(here) = directory {
        let root = here.join("node_modules").join(&package);
        // A package directory that exists but does not answer does NOT end the
        // walk: a workspace often has an empty or partial directory shadowing a
        // real install further up, and stopping there would report "no such
        // module" while the module is installed.
        if root.is_dir() {
            if let Some(found) = inside(&root, &subpath) {
                return Some(settled(found));
            }
        }
        directory = here.parent();
    }
    None
}

/// The package a bare specifier names, and the subpath after it: `("pkg", ".")`
/// from `pkg`, `("pkg", "./deep")` from `pkg/deep`, and the two leading
/// segments for a scoped `@scope/pkg`.
///
/// `.` and `./deep` rather than `""` and `deep` because those are the keys an
/// `"exports"` map is written with, and converting at the boundary means the
/// lookup below never has two spellings of one key.
fn split(specifier: &str) -> Option<(String, String)> {
    let mut parts = specifier.split('/');
    let first = parts.next().filter(|part| !part.is_empty())?;
    let package = match first.starts_with('@') {
        false => first.to_owned(),
        true => format!("{first}/{}", parts.next().filter(|part| !part.is_empty())?),
    };
    let rest: Vec<&str> = parts.collect();
    let subpath = match rest.is_empty() {
        true => ".".to_owned(),
        false => format!("./{}", rest.join("/")),
    };
    Some((package, subpath))
}

/// The file a subpath names inside an installed package.
fn inside(root: &Path, subpath: &str) -> Option<PathBuf> {
    let manifest = read_manifest(root);
    // `"exports"`, when present, is the WHOLE answer in Node — a package with
    // it exposes nothing else, `"main"` included. Honoured that way here too,
    // except that a non-match falls through to the file rules below rather than
    // refusing: see the module doc on the null target.
    if let Some(exports) = manifest.as_ref().and_then(|manifest| manifest.get("exports")) {
        if let Some(target) = target_of(exports, subpath) {
            if let Some(found) = relative(root, &target) {
                return Some(found);
            }
        }
    }
    if subpath != "." {
        return relative(root, subpath);
    }
    // The package root: `"main"`, then `index.*`. `extended` is what "a real
    // file" means in this crate — extension order and `index.*` — and it is
    // called rather than reproduced, so `"main": "./lib"` resolves the same way
    // `import "./lib"` does.
    let main = manifest
        .as_ref()
        .and_then(|manifest| manifest.get("main"))
        .and_then(|main| main.as_str())
        .and_then(|main| relative(root, main));
    main.or_else(|| extended(root.parent()?, root.file_name()?.to_str()?))
}

/// A target written relative to the package root, as a file on disk.
///
/// Refuses anything that is not `./…`: an `"exports"` target may name ANOTHER
/// package (`"./x": "other-pkg/x"`), which is a redirection this resolver does
/// not follow, and treating it as a relative path would answer a file inside
/// the wrong package.
fn relative(root: &Path, target: &str) -> Option<PathBuf> {
    let target = target.strip_prefix("./").unwrap_or(target);
    if target.starts_with('.') || target.contains("..") {
        return None;
    }
    let named = root.join(target);
    match named.is_file() {
        true => Some(named),
        false => extended(root, target),
    }
}

/// `package.json`, parsed — `None` for absent, unreadable or malformed.
///
/// A malformed manifest is not an error here: the file rules below it still
/// answer, which is what `require` does for a package whose `package.json`
/// cannot be read.
fn read_manifest(root: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(root.join("package.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// What an `"exports"` value resolves to for one subpath.
///
/// Three shapes, told apart the way Node tells them apart: a string or an array
/// is a target (and only for `"."`); an object whose first key starts with `.`
/// is a subpath map; any other object is a condition map, resolved for the SAME
/// subpath.
fn target_of(exports: &serde_json::Value, subpath: &str) -> Option<String> {
    match exports {
        serde_json::Value::String(target) => (subpath == ".").then(|| target.clone()),
        // An array is a list of alternatives, "tried in order" — the first that
        // yields a target wins. Whether the FILE exists is decided by the
        // caller, so an alternative naming a missing file is not caught here.
        serde_json::Value::Array(alternatives) => alternatives
            .iter()
            .find_map(|alternative| target_of(alternative, subpath)),
        serde_json::Value::Object(map) => match map.keys().next().is_some_and(|key| key.starts_with('.')) {
            true => subpath_target(map, subpath),
            false => CONDITIONS
                .iter()
                .find_map(|condition| map.get(*condition))
                .and_then(|nested| target_of(nested, subpath)),
        },
        _ => None,
    }
}

/// A subpath map's answer: the exact key, else the most specific `*` pattern.
///
/// "Most specific" is the longest literal prefix before the `*`, which is
/// Node's own rule and is what makes `"./lib/*"` win over `"./*"` for
/// `./lib/one`. Without it the shorter pattern would answer first for every
/// subpath and the longer one would never be reachable.
fn subpath_target(map: &serde_json::Map<String, serde_json::Value>, subpath: &str) -> Option<String> {
    if let Some(found) = map.get(subpath).and_then(|value| target_of(value, ".")) {
        return Some(found);
    }
    let mut best: Option<(usize, String)> = None;
    for (key, value) in map {
        let Some((head, tail)) = key.split_once('*') else { continue };
        let Some(rest) = subpath.strip_prefix(head).and_then(|rest| rest.strip_suffix(tail)) else {
            continue;
        };
        if best.as_ref().is_some_and(|(length, _)| *length >= head.len()) {
            continue;
        }
        // The pattern's own `*` is replaced by what the subpath matched —
        // `"./lib/*": "./dist/*.js"` with `./lib/one` is `./dist/one.js`.
        let Some(target) = target_of(value, ".") else { continue };
        best = Some((head.len(), target.replace('*', rest)));
    }
    best.map(|(_, target)| target)
}
