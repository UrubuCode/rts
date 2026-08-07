//! `module.findPackageJSON(specifier, base)` — the nearest enclosing
//! `package.json`, as a filesystem walk.
//!
//! # Why this one is real when the loader surface around it is not
//!
//! Because it does not load. Node's own documentation says it "only consults
//! the built-in default resolver" and must not be used to decide a module's
//! format — so the answer is a path found by walking directories, which `std`
//! answers completely, and none of the module-table machinery `createRequire`
//! needs is involved.
//!
//! # Not implemented, by name
//!
//! - **The default `base`.** Node defaults it to "the caller's own module
//!   URL". A native is handed a receiver and four values, not a stack of
//!   module frames, so nothing here knows which file called it. Falling back to
//!   the process's working directory — which `rts:runtime`'s `resolve` does,
//!   for a specifier that has no other anchor — would answer a DIFFERENT
//!   `package.json` whenever the caller's file is not under it, which is a
//!   wrong answer that runs. A call with no `base` answers `undefined`.
//! - **A `URL` instance as `specifier` or `base`.** Node accepts
//!   `string | URL`; only a string is read here (a `file://` string included),
//!   because `node:url`'s `URL` is a plain object with data properties and
//!   asking it for its `href` would be reading a property off any object that
//!   happens to have one. `import.meta.url`, which is how this is called, is
//!   already a string.
//! - **`package.json` `"exports"`.** A bare specifier's package root is found
//!   by the `node_modules` walk below; a subpath is NOT re-mapped through the
//!   package's conditional-exports map first, so `findPackageJSON('pkg/deep')`
//!   answers `pkg`'s own `package.json` rather than one an `"exports"` entry
//!   could redirect a deep subpath into. That redirection needs the resolution
//!   algorithm this crate does not have.

use std::path::{Path, PathBuf};

use rts_core_rwk::entry;

/// `module.findPackageJSON(specifier[, base])`.
pub(super) extern "C" fn find_package_json(
    _e: u64,
    _this: u64,
    specifier: u64,
    base: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    // Both read as STRINGS rather than coerced: `text_of` would answer
    // `"[object Object]"` for an object and this would then walk from a
    // directory named after it.
    let read = entry::with_runtime(|context| {
        (
            entry::string_in(context, specifier),
            entry::string_in(context, base),
        )
    });
    let (Some(specifier), Some(base)) = read else {
        return entry::undefined_value();
    };
    let Some(from) = base_path(&base) else {
        return entry::undefined_value();
    };
    let Some(found) = nearest(&specifier, &from) else {
        return entry::undefined_value();
    };
    let text = found.display().to_string();
    entry::with_runtime(|context| entry::make_string(context, &text))
}

/// The file a `base` names — a `file://` URL or a path.
///
/// Through the `url` crate rather than by stripping a prefix: a file URL
/// percent-encodes, and on Windows carries the drive letter after a slash that
/// a hand-rolled strip leaves in place.
///
/// Shared with [`super::require`], whose `createRequire(import.meta.url)` takes
/// an argument of exactly the same `string | URL-as-text` shape — two readings
/// of one convention would differ first on Windows, where it is hardest to see.
pub(super) fn base_path(base: &str) -> Option<PathBuf> {
    match base.starts_with("file:") {
        true => url::Url::parse(base).ok()?.to_file_path().ok(),
        false => Some(PathBuf::from(base)),
    }
}

/// The `package.json` a specifier resolves under, from the file that named it.
fn nearest(specifier: &str, from: &Path) -> Option<PathBuf> {
    // A module the host provides has no `package.json`, and looking for one
    // would walk the filesystem from the caller's directory and answer whatever
    // happened to be up there — an unrelated package, reported for `node:fs`.
    if specifier.starts_with("node:") || super::is_provided(specifier) {
        return None;
    }
    let directory = from.parent().unwrap_or(Path::new("."));
    let relative = specifier.starts_with("./") || specifier.starts_with("../");
    if relative || Path::new(specifier).is_absolute() {
        let target = match relative {
            true => directory.join(specifier),
            false => PathBuf::from(specifier),
        };
        // The specifier need not exist — Node answers the enclosing package of
        // where it WOULD be — so a target that is not a directory is treated as
        // a file and the walk starts at its parent.
        let start = match target.is_dir() {
            true => target.clone(),
            false => target.parent().unwrap_or(directory).to_path_buf(),
        };
        return walk_up(&start, |directory| {
            candidate(directory.join("package.json"))
        });
    }
    // A bare specifier: the package's own `package.json`, found the way an
    // import of it would be — `node_modules` directories from here upwards.
    let package = package_name(specifier)?;
    walk_up(directory, |directory| {
        candidate(directory.join("node_modules").join(&package).join("package.json"))
    })
}

/// The package a bare specifier names: `pkg` from `pkg/deep`, and the two
/// leading segments of a scoped `@scope/pkg/deep`.
fn package_name(specifier: &str) -> Option<String> {
    let mut parts = specifier.split('/');
    let first = parts.next().filter(|part| !part.is_empty())?;
    match first.starts_with('@') {
        false => Some(first.to_owned()),
        true => parts
            .next()
            .filter(|part| !part.is_empty())
            .map(|second| format!("{first}/{second}")),
    }
}

/// A path, if a file is actually there.
fn candidate(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then_some(path)
}

/// Each directory from `start` to the filesystem root, until one answers.
fn walk_up(start: &Path, mut ask: impl FnMut(&Path) -> Option<PathBuf>) -> Option<PathBuf> {
    let mut here = Some(start);
    while let Some(directory) = here {
        if let Some(found) = ask(directory) {
            return Some(found);
        }
        here = directory.parent();
    }
    None
}
