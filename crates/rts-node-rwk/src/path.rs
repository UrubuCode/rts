//! `node:path` — joining and taking apart a path, as text.
//!
//! # Why this is string work and not `std::path`
//!
//! Because Node's `path` is defined on STRINGS, not on what the file system
//! accepts. `path.join("a", "..")` answers `"."` without asking whether `a`
//! exists, and `path.extname(".gitignore")` is `""` rather than `".gitignore"`.
//! Routing that through `std::path::PathBuf` gets several of those wrong in the
//! direction that looks right — and one of them, `PathBuf::push` with an
//! absolute second component, silently discards the first.
//!
//! # Which separator
//!
//! `posix` semantics, with `\` accepted as a separator on the way in. Node has
//! `path.win32` and `path.posix` as separate objects and picks one for the bare
//! export by platform; this answers `/` everywhere, which is what the corpus
//! this serves compares against. Named rather than silent: a program asserting
//! `path.sep === "\\"` on Windows fails here and is right to.
//!
//! # Not implemented, by name
//!
//! `relative`, `parse`, `format`, `toNamespacedPath`, `matchesGlob`, and the
//! `win32`/`posix` sub-objects. Each answers `undefined`, which a program sees,
//! rather than a plausible wrong answer.

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:path` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("join", join),
        ("resolve", resolve),
        ("normalize", normalize_),
        ("basename", basename),
        ("dirname", dirname),
        ("extname", extname),
        ("isAbsolute", is_absolute),
    ];
    let namespace = rts_core_rwk::entry::make_namespace(context, members);
    let separator = rts_core_rwk::entry::make_string(context, "/");
    rts_core_rwk::entry::put_member(context, namespace, "sep", separator);
    let delimiter = rts_core_rwk::entry::make_string(context, ";");
    rts_core_rwk::entry::put_member(context, namespace, "delimiter", delimiter);
    namespace
}

/// `path.join(a, b, c, d)` — four, because the convention carries four slots.
///
/// A call with more is refused at the site rather than losing its arguments
/// here, which is the same trade `Object.assign` and `Function.prototype.call`
/// already make.
extern "C" fn join(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let parts: Vec<String> = [a, b, c, d].into_iter().filter_map(text).collect();
    let joined = parts
        .iter()
        .filter(|part| !part.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("/");
    // An empty join is `"."`, not `""`: Node answers the current directory,
    // and `""` would be a path that opens nothing.
    let answer = match joined.is_empty() {
        true => ".".to_owned(),
        false => normalize(&joined),
    };
    string(&answer)
}

/// `path.resolve(a, b, c, d)` — right to left until something is absolute.
///
/// The current directory is asked for only when nothing given is absolute,
/// which is what makes `resolve("/a", "b")` independent of where the process is.
extern "C" fn resolve(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let parts: Vec<String> = [a, b, c, d].into_iter().filter_map(text).collect();
    let mut out = String::new();
    for part in parts.iter().rev() {
        if part.is_empty() {
            continue;
        }
        out = match out.is_empty() {
            true => part.clone(),
            false => format!("{part}/{out}"),
        };
        if absolute(part) {
            return string(&normalize(&out));
        }
    }
    let here = std::env::current_dir()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| ".".to_owned());
    let joined = match out.is_empty() {
        true => here,
        false => format!("{here}/{out}"),
    };
    string(&normalize(&joined))
}

/// `path.normalize(p)`.
extern "C" fn normalize_(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    match text(value) {
        Some(path) => string(&normalize(&path)),
        None => rts_core_rwk::entry::undefined_value(),
    }
}

/// `path.basename(p, ext?)` — the last component, with an extension removed.
extern "C" fn basename(_e: u64, _this: u64, value: u64, ext: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(value) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let trimmed = path.trim_end_matches(['/', '\\']);
    let last = trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(trimmed)
        .to_owned();
    // The suffix comes off only when the name is not ONLY the suffix:
    // `basename(".ts", ".ts")` is `".ts"` in Node, not `""`.
    let answer = match text(ext) {
        Some(suffix) if last != suffix && last.ends_with(&suffix) => {
            last[..last.len() - suffix.len()].to_owned()
        }
        _ => last,
    };
    string(&answer)
}

/// `path.dirname(p)`.
extern "C" fn dirname(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(value) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let trimmed = path.trim_end_matches(['/', '\\']);
    let answer = match trimmed.rfind(['/', '\\']) {
        // A path with no separator has `"."` as its directory, which is not the
        // same as the empty string: one is a directory and the other is not a
        // path at all.
        None => ".".to_owned(),
        Some(0) => "/".to_owned(),
        Some(at) => trimmed[..at].to_owned(),
    };
    string(&answer)
}

/// `path.extname(p)`.
///
/// A leading dot is not an extension: `extname(".gitignore")` is `""`. That is
/// the case an implementation written as "everything after the last dot" gets
/// wrong, and the one programs actually hit.
extern "C" fn extname(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(value) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let last = path.rsplit(['/', '\\']).next().unwrap_or(&path);
    let answer = match last.rfind('.') {
        Some(0) | None => String::new(),
        Some(at) => last[at..].to_owned(),
    };
    string(&answer)
}

/// `path.isAbsolute(p)`.
extern "C" fn is_absolute(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    bool_value(text(value).is_some_and(|path| absolute(&path)))
}

/// Whether a path starts at a root.
///
/// A drive letter counts, because a Windows path handed to this on Windows is
/// what a program actually has — even though the separator answered is `/`.
fn absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    if path.starts_with('/') || path.starts_with('\\') {
        return true;
    }
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'/' | b'\\')
}

/// Collapses `.`, `..` and repeated separators.
///
/// `..` past the root is dropped for an absolute path and KEPT for a relative
/// one: `normalize("/../a")` is `"/a"` and `normalize("../a")` is `"../a"`,
/// because a relative path's parent is not knowable here.
fn normalize(path: &str) -> String {
    let rooted = absolute(path);
    let mut out: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => match out.last() {
                Some(&last) if last != ".." => {
                    out.pop();
                }
                _ if rooted => {}
                _ => out.push(".."),
            },
            other => out.push(other),
        }
    }
    let joined = out.join("/");
    match (rooted, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}

/// An argument as text.
fn text(value: u64) -> Option<String> {
    let absent = rts_core_rwk::entry::undefined_value();
    match value == absent {
        true => None,
        false => rts_core_rwk::entry::text_of(value),
    }
}

/// A string value.
fn string(text: &str) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, text))
}

/// A boolean value.
fn bool_value(held: bool) -> u64 {
    rts_core_rwk::entry::boolean_value(held)
}
