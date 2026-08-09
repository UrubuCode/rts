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
//! The bare export (`path.join`, …) answers `/` everywhere, matching
//! `path.posix` — that is the corpus this serves compares against, and it is
//! named rather than silent: a program asserting `path.sep === "\\"` on
//! Windows fails here and is right to. `path.win32` is a genuinely separate
//! implementation, always available regardless of host, exactly as Node's own
//! `path.win32` is — both flavors share one generic core parameterized on the
//! separator character, since the algorithm (join/normalize/resolve's segment
//! processing) does not otherwise differ between them.
//!
//! # Not implemented, by name
//!
//! `matchesGlob` is now here, and this note used to say why it was not: a
//! glob-to-`RegExp` compiler needs a way to construct a `RegExp`, which the API
//! this crate is restricted to does not expose. That reasoning is what changed
//! rather than the API — translating a glob into a pattern means escaping every
//! metacharacter a path may legally contain, and a list that misses one is a
//! pattern matching something it must not. Matching directly has no such list.
//!
//! What it covers is `*` (within a segment), `**` (across separators) and `?`.
//! Brace expansion and character classes are NOT covered, and a pattern using
//! them answers false — the same answer a genuinely non-matching pattern gives,
//! which is why the gap is named here rather than left to be discovered.
//!
//! `win32`'s UNC-path root/`toNamespacedPath` handling covers the common
//! `\\server\share\...` shape only — Node's own long-path edge cases
//! (device paths, `\\?\` inputs handed back in) are not separately verified
//! here; see the module test plan in `docs/reference/node/path.md` §7 for
//! what is flagged `(verify)` even in the specification itself.

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:path` is (posix-flavored — see the module doc).
pub fn namespace(context: &mut Context) -> u64 {
    let posix = flavor_namespace(context, '/');
    let win32 = flavor_namespace(context, '\\');
    // The bare export IS the posix flavor's function set, plus both children
    // reachable off it — matching Node's `path.posix === path` on POSIX hosts
    // in spirit, without pretending this engine detects a host at all.
    let members: &[(&str, Provided)] = &[
        ("join", join_posix),
        ("resolve", resolve_posix),
        ("normalize", normalize_posix),
        ("basename", basename),
        ("dirname", dirname),
        ("extname", extname),
        ("isAbsolute", is_absolute),
        ("relative", relative_posix),
        ("parse", parse_posix),
        ("format", format_posix),
        ("toNamespacedPath", to_namespaced_posix),
        ("matchesGlob", matches_glob_entry),
    ];
    let namespace = rts_core_rwk::entry::make_namespace(context, members);
    let separator = rts_core_rwk::entry::make_string(context, "/");
    rts_core_rwk::entry::put_member(context, namespace, "sep", separator);
    let delimiter = rts_core_rwk::entry::make_string(context, ":");
    rts_core_rwk::entry::put_member(context, namespace, "delimiter", delimiter);
    rts_core_rwk::entry::put_member(context, namespace, "posix", posix);
    rts_core_rwk::entry::put_member(context, namespace, "win32", win32);
    rts_core_rwk::entry::put_member(context, posix, "posix", posix);
    rts_core_rwk::entry::put_member(context, posix, "win32", win32);
    rts_core_rwk::entry::put_member(context, win32, "posix", posix);
    rts_core_rwk::entry::put_member(context, win32, "win32", win32);
    namespace
}

/// One flavor's namespace — `path.posix` or `path.win32` — built from the
/// same function set at a different separator.
fn flavor_namespace(context: &mut Context, sep: char) -> u64 {
    let members: &[(&str, Provided)] = if sep == '\\' {
        &[
            ("join", join_win32),
            ("resolve", resolve_win32),
            ("normalize", normalize_win32),
            ("basename", basename),
            ("dirname", dirname),
            ("extname", extname),
            ("isAbsolute", is_absolute),
            ("relative", relative_win32),
            ("parse", parse_win32),
            ("format", format_win32),
            ("toNamespacedPath", to_namespaced_win32),
            ("matchesGlob", matches_glob_entry),
        ]
    } else {
        &[
            ("join", join_posix),
            ("resolve", resolve_posix),
            ("normalize", normalize_posix),
            ("basename", basename),
            ("dirname", dirname),
            ("extname", extname),
            ("isAbsolute", is_absolute),
            ("relative", relative_posix),
            ("parse", parse_posix),
            ("format", format_posix),
            ("toNamespacedPath", to_namespaced_posix),
            ("matchesGlob", matches_glob_entry),
        ]
    };
    let namespace = rts_core_rwk::entry::make_namespace(context, members);
    let separator = rts_core_rwk::entry::make_string(context, &sep.to_string());
    rts_core_rwk::entry::put_member(context, namespace, "sep", separator);
    let delimiter_text = if sep == '\\' { ";" } else { ":" };
    let delimiter = rts_core_rwk::entry::make_string(context, delimiter_text);
    rts_core_rwk::entry::put_member(context, namespace, "delimiter", delimiter);
    namespace
}

/// `path.join(a, b, ...)` — past four, the compiler's spilled vector.
///
/// The doc here used to claim a fifth argument was "refused at the site
/// rather than losing its arguments here" — it was not refused, it was
/// silently dropped: `path.join('a','b','c','d','e')` answered a path built
/// from the first four only. `arguments_variadic` reads the same spilled
/// vector `String.fromCharCode` and `Math.max` do, the way `rts-core-rwk`'s
/// `arguments_at` states it once for every native past the four slots.
extern "C" fn join_posix(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    string(&join_g(&arguments_variadic(a, b, c, d), '/'))
}
extern "C" fn join_win32(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    string(&join_g(&arguments_variadic(a, b, c, d), '\\'))
}

/// `path.resolve(a, b, ...)` — right to left until something is absolute.
extern "C" fn resolve_posix(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    string(&resolve_g(&arguments_variadic(a, b, c, d), '/'))
}
extern "C" fn resolve_win32(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    string(&resolve_g(&arguments_variadic(a, b, c, d), '\\'))
}

/// `path.normalize(p)`.
extern "C" fn normalize_posix(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    match text(value) {
        Some(path) => string(&normalize_g(&path, '/')),
        None => rts_core_rwk::entry::undefined_value(),
    }
}
extern "C" fn normalize_win32(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    match text(value) {
        Some(path) => string(&normalize_g(&path, '\\')),
        None => rts_core_rwk::entry::undefined_value(),
    }
}

/// `path.relative(from, to)`.
extern "C" fn relative_posix(_e: u64, _this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    match (text(from), text(to)) {
        (Some(f), Some(t)) => string(&relative_g(&f, &t, '/')),
        _ => rts_core_rwk::entry::undefined_value(),
    }
}
extern "C" fn relative_win32(_e: u64, _this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    match (text(from), text(to)) {
        (Some(f), Some(t)) => string(&relative_g(&f, &t, '\\')),
        _ => rts_core_rwk::entry::undefined_value(),
    }
}

/// `path.parse(p)` — `{ root, dir, base, ext, name }`.
extern "C" fn parse_posix(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    parse_g(value, '/')
}
extern "C" fn parse_win32(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    parse_g(value, '\\')
}
fn parse_g(value: u64, sep: char) -> u64 {
    let Some(path) = text(value) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let root = root_g(&path, sep);
    let dir = dirname_str(&path);
    let base = basename_str(&path, None);
    let (name, ext) = split_ext(&base);
    rts_core_rwk::entry::with_runtime(|context| {
        let object = rts_core_rwk::entry::make_object(context);
        for (key, value) in [
            ("root", root.as_str()),
            ("dir", dir.as_str()),
            ("base", base.as_str()),
            ("ext", ext.as_str()),
            ("name", name.as_str()),
        ] {
            let held = rts_core_rwk::entry::make_string(context, value);
            rts_core_rwk::entry::put_member(context, object, key, held);
        }
        object
    })
}

/// `path.format(pathObject)`.
extern "C" fn format_posix(_e: u64, _this: u64, obj: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    format_g(obj, '/')
}
extern "C" fn format_win32(_e: u64, _this: u64, obj: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    format_g(obj, '\\')
}
fn format_g(obj: u64, sep: char) -> u64 {
    // The five member reads happen inside ONE borrow — `get_member` is a
    // context-taking form, safe here — and every text conversion happens
    // AFTER that borrow ends, because `text_of` is ambient and would nest.
    let (dir_v, root_v, base_v, ext_v, name_v) = rts_core_rwk::entry::with_runtime(|context| {
        (
            rts_core_rwk::entry::get_member(context, obj, "dir"),
            rts_core_rwk::entry::get_member(context, obj, "root"),
            rts_core_rwk::entry::get_member(context, obj, "base"),
            rts_core_rwk::entry::get_member(context, obj, "ext"),
            rts_core_rwk::entry::get_member(context, obj, "name"),
        )
    });
    let composed = compose_format(text(dir_v), text(root_v), text(base_v), text(ext_v), text(name_v), sep);
    string(&composed)
}

/// `path.toNamespacedPath(p)` — a no-op on the posix flavor.
extern "C" fn to_namespaced_posix(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    value
}
/// `path.win32.toNamespacedPath(p)` — the `\\?\`/`\\?\UNC\` long-path prefix.
extern "C" fn to_namespaced_win32(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(value) else {
        return value;
    };
    if let Some(rest) = path.strip_prefix("\\\\").or_else(|| path.strip_prefix("//")) {
        return string(&format!("\\\\?\\UNC\\{rest}"));
    }
    if drive_absolute(&path) {
        return string(&format!("\\\\?\\{path}"));
    }
    string(&path)
}

/// `path.basename(p, ext?)` — the last component, with an extension removed.
extern "C" fn basename(_e: u64, _this: u64, value: u64, ext: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(value) else {
        return rts_core_rwk::entry::undefined_value();
    };
    string(&basename_str(&path, text(ext)))
}

fn basename_str(path: &str, suffix: Option<String>) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let last = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed).to_owned();
    // The suffix comes off only when the name is not ONLY the suffix:
    // `basename(".ts", ".ts")` is `".ts"` in Node, not `""`.
    match suffix {
        Some(suffix) if last != suffix && last.ends_with(&suffix) => {
            last[..last.len() - suffix.len()].to_owned()
        }
        _ => last,
    }
}

/// `path.dirname(p)`.
extern "C" fn dirname(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(value) else {
        return rts_core_rwk::entry::undefined_value();
    };
    string(&dirname_str(&path))
}

fn dirname_str(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        // A path with no separator has `"."` as its directory, which is not the
        // same as the empty string: one is a directory and the other is not a
        // path at all.
        None => ".".to_owned(),
        Some(0) => trimmed[..1].to_owned(),
        Some(at) => trimmed[..at].to_owned(),
    }
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
    let base = basename_str(&path, None);
    string(&split_ext(&base).1)
}

/// A base name split into `(name, ext)`, `extname`'s rule applied to both.
fn split_ext(base: &str) -> (String, String) {
    match base.rfind('.') {
        Some(0) | None => (base.to_owned(), String::new()),
        Some(at) => (base[..at].to_owned(), base[at..].to_owned()),
    }
}

/// `path.isAbsolute(p)`.
extern "C" fn is_absolute(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    bool_value(text(value).is_some_and(|path| absolute(&path)))
}

/// Whether a path starts at a root.
///
/// A drive letter counts, because a Windows path handed to this on Windows is
/// what a program actually has — even though the bare export's separator is
/// `/`.
fn absolute(path: &str) -> bool {
    if path.starts_with('/') || path.starts_with('\\') {
        return true;
    }
    drive_absolute(path)
}

/// Whether a path starts with a drive letter and a root separator (`C:\`,
/// `C:/`) — drive-*relative* (`C:foo`, no separator) does not count.
fn drive_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'/' | b'\\')
}

/// The absolute prefix of a path, in the given separator's spelling — `""`
/// for a relative path, `/` (or `\`) for a plain root, `C:\` for a drive
/// root. UNC roots (`\\server\share\`) are approximated as a bare `sep`
/// rather than the full server/share prefix — a named simplification, not a
/// silent one: `path.win32.parse('\\\\s\\share\\f')`'s `root` is narrower
/// than Node's here.
fn root_g(path: &str, sep: char) -> String {
    if drive_absolute(path) {
        let letter = (path.as_bytes()[0] as char).to_ascii_uppercase();
        return format!("{letter}:{sep}");
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return sep.to_string();
    }
    String::new()
}

/// Collapses `.`, `..` and repeated separators, emitting `sep` throughout.
///
/// `..` past the root is dropped for an absolute path and KEPT for a relative
/// one: `normalize("/../a")` is `"/a"` and `normalize("../a")` is `"../a"`,
/// because a relative path's parent is not knowable here.
fn normalize_g(path: &str, sep: char) -> String {
    let root = root_g(path, sep);
    let rooted = !root.is_empty();
    // A drive prefix (`C:`) is two characters with no separator between them
    // and no drive letter to lose — strip it before splitting so it does not
    // leak into the component list as an ordinary segment.
    let body = if drive_absolute(path) { &path[2..] } else { path };
    let mut out: Vec<&str> = Vec::new();
    for part in body.split(['/', '\\']) {
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
    let joined = out.join(&sep.to_string());
    let normalized = match (rooted, joined.is_empty()) {
        (true, _) => format!("{root}{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    };
    // A trailing separator is KEPT, because it is the difference between naming
    // a directory and naming a thing: `normalize("C:\\temp\\foo\\")` answers a
    // path that still ends in one, and Node's own tests assert it. Not added
    // where there was none, and not added to a bare root — `"/"` already ends in
    // one and `"."` never had one.
    let trailed = path.ends_with(['/', '\\']) && !normalized.ends_with(sep) && normalized != ".";
    match trailed {
        true => format!("{normalized}{sep}"),
        false => normalized,
    }
}

/// `path.matchesGlob(path, pattern)` — whether a path matches a glob.
///
/// The three wildcards a path glob has, and nothing else: `*` matches within one
/// segment, `**` matches across separators, and `?` matches one character.
/// Brace expansion and character classes are NOT here, and that is a named gap
/// rather than a silent one — a pattern using them answers false, which is the
/// same answer a pattern that genuinely does not match gives, so this comment is
/// where a reader finds out.
///
/// Written as a matcher rather than by compiling to the regular expression
/// engine this crate already links: the translation has to escape every
/// metacharacter a path may legally contain, and getting that list wrong is a
/// pattern that matches something it should not.
fn matches_glob(path: &str, pattern: &str) -> bool {
    fn matches(path: &[u8], pattern: &[u8]) -> bool {
        match (path.first(), pattern.first()) {
            (None, None) => true,
            // A trailing `**` matches nothing left as well as something.
            (None, Some(_)) => pattern == b"**" || pattern == b"*",
            (Some(_), None) => false,
            (Some(&here), Some(b'*')) => {
                let across = pattern.starts_with(b"**");
                let rest = match across {
                    true => &pattern[2..],
                    false => &pattern[1..],
                };
                // Consume nothing, or one character and try again — the second
                // only while the star is allowed to cross what it is looking at.
                matches(path, rest)
                    || ((across || here != b'/') && matches(&path[1..], pattern))
            }
            (Some(&here), Some(b'?')) if here != b'/' => matches(&path[1..], &pattern[1..]),
            (Some(&here), Some(&want)) if here == want => matches(&path[1..], &pattern[1..]),
            _ => false,
        }
    }
    matches(path.as_bytes(), pattern.as_bytes())
}

/// `path.matchesGlob(path, pattern)`.
extern "C" fn matches_glob_entry(
    _e: u64,
    _this: u64,
    path: u64,
    pattern: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let path = rts_core_rwk::entry::text_of(path).unwrap_or_default();
    let pattern = rts_core_rwk::entry::text_of(pattern).unwrap_or_default();
    rts_core_rwk::entry::boolean_value(matches_glob(&path, &pattern))
}

/// `path.join`'s core: filter empties, join on `sep`, normalize.
fn join_g(parts: &[String], sep: char) -> String {
    let joined = parts
        .iter()
        .filter(|part| !part.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(&sep.to_string());
    // An empty join is `"."`, not `""`: Node answers the current directory,
    // and `""` would be a path that opens nothing.
    match joined.is_empty() {
        true => ".".to_owned(),
        false => normalize_g(&joined, sep),
    }
}

/// `path.resolve`'s core: right to left until something is absolute, else
/// the process's current directory.
fn resolve_g(parts: &[String], sep: char) -> String {
    let mut out = String::new();
    for part in parts.iter().rev() {
        if part.is_empty() {
            continue;
        }
        out = match out.is_empty() {
            true => part.clone(),
            false => format!("{part}{sep}{out}"),
        };
        if absolute(part) {
            return without_trailing(&normalize_g(&out, sep), sep);
        }
    }
    let here = std::env::current_dir()
        .map(|path| {
            path.to_string_lossy()
                .replace(['/', '\\'], &sep.to_string())
        })
        .unwrap_or_else(|_| ".".to_owned());
    let joined = match out.is_empty() {
        true => here,
        false => format!("{here}{sep}{out}"),
    };
    without_trailing(&normalize_g(&joined, sep), sep)
}

/// A path with its trailing separator removed, unless that separator is all of it.
///
/// `resolve` DROPS one where `normalize` keeps it: `resolve("/foo", "/tmp/f/")`
/// is `"/tmp/f"`. The two differ on purpose — one answers "what does this path
/// mean" and the other "what is the absolute path of this thing" — and a root
/// keeps its separator because there it is the whole path.
fn without_trailing(path: &str, sep: char) -> String {
    match path.len() > 1 && path.ends_with(sep) {
        true => path[..path.len() - 1].to_owned(),
        false => path.to_owned(),
    }
}

/// `path.relative`'s core: both sides resolved, then diffed component-wise.
fn relative_g(from: &str, to: &str, sep: char) -> String {
    let rf = resolve_g(&[from.to_owned()], sep);
    let rt = resolve_g(&[to.to_owned()], sep);
    if rf == rt {
        return String::new();
    }
    let from_parts: Vec<&str> = rf.split(sep).filter(|part| !part.is_empty()).collect();
    let to_parts: Vec<&str> = rt.split(sep).filter(|part| !part.is_empty()).collect();
    let mut common = 0;
    while common < from_parts.len() && common < to_parts.len() && from_parts[common] == to_parts[common] {
        common += 1;
    }
    let mut out: Vec<String> = Vec::new();
    for _ in common..from_parts.len() {
        out.push("..".to_owned());
    }
    out.extend(to_parts[common..].iter().map(|part| (*part).to_owned()));
    out.join(&sep.to_string())
}

/// `path.format`'s composition rule — `dir`/`root` for the head,
/// `base` or `name`+`ext` for the tail, `ext` dot-normalized.
fn compose_format(
    dir: Option<String>,
    root: Option<String>,
    base: Option<String>,
    ext: Option<String>,
    name: Option<String>,
    sep: char,
) -> String {
    let head = dir.clone().or_else(|| root.clone()).unwrap_or_default();
    let tail = match base {
        Some(base) if !base.is_empty() => base,
        _ => {
            let name = name.unwrap_or_default();
            match ext {
                Some(ext) if !ext.is_empty() => match ext.starts_with('.') {
                    true => format!("{name}{ext}"),
                    false => format!("{name}.{ext}"),
                },
                _ => name,
            }
        }
    };
    if head.is_empty() {
        return tail;
    }
    match Some(&head) == root.as_ref() {
        true => format!("{head}{tail}"),
        false => format!("{head}{sep}{tail}"),
    }
}

/// The call's arguments, as present text, in order — past four the
/// compiler's spilled vector rather than the four convention slots alone.
///
/// `rts_core_rwk::entry::arguments_at` is the one place that reads the
/// spilled vector back; this crate is a caller of it rather than a second
/// copy, the same way `Math.max` and `a.push` already are inside
/// `rts-core-rwk` itself.
fn arguments_variadic(a: u64, b: u64, c: u64, d: u64) -> Vec<String> {
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::arguments_at(context, 0, [a, b, c, d]))
        .into_iter()
        .filter_map(text)
        .collect()
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
