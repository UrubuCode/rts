//! node:path — base extern "C" symbol implementations (the sync surface).
//!
//! ABI mirrors the pure-namespace shape used across RTS: `Str` args arrive as
//! `(ptr, len)` and are rebuilt via `from_abi` (`None` on null / invalid
//! UTF-8, mapped to the ABI's error sentinel — `0`/empty — never a panic);
//! string results are interned to GC string handles. Symbols follow the
//! rts-node convention `__RTS_FN_NODE_PATH_*`.
//!
//! `join`/`normalize`/`dirname`/`basename`/`extname`/`isAbsolute`/`parse` are
//! pure string manipulation (no `fs` access, matching Node); `resolve`/
//! `relative`/`toNamespacedPath` additionally read `process.cwd()` (via
//! `std::env::current_dir`), exactly like Node's own implementation does.
//! `parse` returns a real shaped object via `alloc_shaped_object` (not a
//! string/bool/void scalar like the rest of this file).

use std::path::{Component, Path, PathBuf};

use rts_engine::abi::str_abi::from_abi;
use rts_engine::heap::shapes::{alloc_shaped_object, string_word};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Interns a Rust string as a GC string handle (the ABI `Handle` return).
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

const SEP: char = std::path::MAIN_SEPARATOR;

fn is_sep_char(c: char) -> bool {
    c == '/' || c == SEP
}

/// Mirrors Node's `path.normalize`: collapses repeated separators, drops `.`
/// segments, resolves `..` against what's already been collapsed (a `..`
/// that would escape an already-reached filesystem root is discarded —
/// matches Node: `path.normalize('/../x') === '/x'`, not `'/../x'`),
/// preserves a trailing separator when the input had one, and maps the
/// empty string to `"."`.
fn normalize_str(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let had_trailing_sep = path.chars().last().map(is_sep_char).unwrap_or(false);
    let mut out = PathBuf::new();
    for comp in Path::new(path).components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {
                    // Already at a filesystem root — a `..` here is a no-op.
                }
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    let mut rendered = out.to_string_lossy().into_owned();
    if rendered.is_empty() {
        rendered = ".".to_string();
    }
    if had_trailing_sep && !rendered.ends_with(SEP) {
        rendered.push(SEP);
    }
    rendered
}

fn normalize_pathbuf(p: &Path) -> PathBuf {
    PathBuf::from(normalize_str(&p.to_string_lossy()))
}

/// `path.join(a, b)` — concatenate with the platform separator, then
/// normalize (Node's `join` always normalizes the joined result).
fn join2(a: &str, b: &str) -> String {
    let joined = if a.is_empty() {
        b.to_string()
    } else if b.is_empty() {
        a.to_string()
    } else if a.chars().last().map(is_sep_char).unwrap_or(false) {
        format!("{a}{b}")
    } else {
        format!("{a}{SEP}{b}")
    };
    if joined.is_empty() {
        ".".to_string()
    } else {
        normalize_str(&joined)
    }
}

fn absolutize(path: &str, cwd: &Path) -> PathBuf {
    if path.is_empty() {
        cwd.to_path_buf()
    } else {
        // `PathBuf::join`/`push` already implement exactly the override
        // rules we need: a fully absolute `path` (POSIX `/x`, or Windows
        // `C:\x` / `\\server\share`) replaces `cwd` outright; a Windows
        // *root-relative* path (`/x` with a root but no drive prefix) keeps
        // `cwd`'s drive letter and replaces only the rest — which is exactly
        // how Node resolves a root-relative path against the current drive.
        cwd.join(path)
    }
}

/// `path.resolve(a, b)` — Node folds segments right-to-left, letting the
/// rightmost absolute segment discard everything to its left, then
/// normalizes once; the result is always absolute. Folding
/// `cwd.join(a).join(b)` left-to-right is equivalent: `PathBuf::join`
/// already discards the accumulator when the next segment is absolute (and,
/// on Windows, keeps the accumulator's drive letter for a root-relative
/// segment), so the rightmost absolute/root-relative segment always wins,
/// matching Node exactly while getting the Windows drive-letter borrowing
/// for free from `std::path`.
fn resolve2(a: &str, b: &str) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut acc = cwd;
    for seg in [a, b] {
        if !seg.is_empty() {
            acc = acc.join(seg);
        }
    }
    let normalized = normalize_str(&acc.to_string_lossy());
    if normalized.is_empty() {
        SEP.to_string()
    } else {
        normalized
    }
}

/// `path.dirname(path)` — directory portion (everything before the final
/// component); `"."` when there is none, the root itself when `path` already
/// is one (e.g. `dirname('/') === '/'`). Uses `has_root()`, not
/// `is_absolute()`: Node's own `dirname`/`isAbsolute` treat a leading
/// separator as "already at a root" even without a Windows drive letter
/// (`dirname('/') === '/'` holds on win32 too), and Rust's `is_absolute()`
/// alone would say `false` for a driveless root on Windows.
fn dirname(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let p = Path::new(path);
    match p.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_string_lossy().into_owned(),
        _ => {
            if p.has_root() {
                p.to_string_lossy().into_owned()
            } else {
                ".".to_string()
            }
        }
    }
}

/// `path.basename(path)` — final path component; `""` for a bare root
/// (`basename('/') === ''`, matching Node).
fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `path.basename(path, suffix)` split out as a fixed-arity fn: strips a
/// trailing `suffix` from the basename (returning `""` when the basename
/// equals `suffix` exactly — `basename('/foo/bar.html', 'bar.html') === ''`,
/// matching Node), matching Node's optional 2nd arg.
fn basename_ext(path: &str, suffix: &str) -> String {
    let base = basename(path);
    if suffix.is_empty() || suffix.len() > base.len() {
        return base;
    }
    if base == suffix {
        return String::new();
    }
    match base.strip_suffix(suffix) {
        Some(stripped) => stripped.to_string(),
        None => base,
    }
}

/// `path.extname(path)` — extension INCLUDING the leading dot; `""` when
/// absent (hidden dotfiles like `.gitignore` have no extension, matching
/// Node: `extname('.gitignore') === ''`, `extname('index.') === '.'`).
fn extname(path: &str) -> String {
    match Path::new(path).extension() {
        Some(ext) => format!(".{}", ext.to_string_lossy()),
        None => String::new(),
    }
}

/// `path.relative(from, to)` — both sides are resolved against `cwd` (like
/// Node's `resolve`), then the common prefix is stripped and replaced by the
/// right number of `..` climbs. Falls back to returning the absolute `to`
/// when the two sides don't share a root (e.g. different Windows drives) —
/// there is no relative path between them, same as Node.
fn relative(from: &str, to: &str) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let from_abs = normalize_pathbuf(&absolutize(from, &cwd));
    let to_abs = normalize_pathbuf(&absolutize(to, &cwd));
    if from_abs == to_abs {
        return String::new();
    }
    let from_comps: Vec<Component> = from_abs.components().collect();
    let to_comps: Vec<Component> = to_abs.components().collect();
    let root_len = |c: &[Component]| {
        c.iter()
            .take_while(|x| matches!(x, Component::Prefix(_) | Component::RootDir))
            .count()
    };
    let from_root = root_len(&from_comps);
    let to_root = root_len(&to_comps);
    if from_comps[..from_root] != to_comps[..to_root] {
        return to_abs.to_string_lossy().into_owned();
    }
    let from_body = &from_comps[from_root..];
    let to_body = &to_comps[to_root..];
    let mut i = 0;
    while i < from_body.len() && i < to_body.len() && from_body[i] == to_body[i] {
        i += 1;
    }
    let mut parts: Vec<String> = Vec::with_capacity((from_body.len() - i) + (to_body.len() - i));
    for _ in i..from_body.len() {
        parts.push("..".to_string());
    }
    for c in &to_body[i..] {
        parts.push(c.as_os_str().to_string_lossy().into_owned());
    }
    parts.join(&SEP.to_string())
}

/// Root portion of `path` (the leading drive/UNC prefix + root separator, or
/// the bare root separator on POSIX): `""` for a relative path, `"/"` for a
/// POSIX-absolute path, `"C:\\"` / `"\\\\server\\share\\"` for a Windows
/// drive/UNC root. Walks `Path::components()` and stops at the first
/// `Normal` component — exactly the leading `Prefix`/`RootDir` run Node's own
/// `path.parse(...).root` captures.
fn root_of(path: &str) -> String {
    let mut root = String::new();
    for comp in Path::new(path).components() {
        match comp {
            Component::Prefix(prefix) => root.push_str(&prefix.as_os_str().to_string_lossy()),
            Component::RootDir => root.push(SEP),
            _ => break,
        }
    }
    root
}

/// `path.parse(path)` — splits `path` into `{ root, dir, base, ext, name }`,
/// Node-style. `""` maps to all-empty fields (Node: `path.parse('')` is `{
/// root: '', dir: '', base: '', ext: '', name: '' }` — unlike bare
/// `dirname('')`, which is `"."`, so this does NOT delegate to `dirname` for
/// the empty case). `ext`/`base`/`name` reuse the same `extname`/`basename`
/// helpers backing the standalone `path.extname`/`path.basename` members, so
/// the two surfaces never disagree on a given input.
fn parse_path(path: &str) -> (String, String, String, String, String) {
    if path.is_empty() {
        return (String::new(), String::new(), String::new(), String::new(), String::new());
    }
    let root = root_of(path);
    let dir = dirname(path);
    let base = basename(path);
    let ext = extname(path);
    let name = if ext.is_empty() {
        base.clone()
    } else {
        base[..base.len() - ext.len()].to_string()
    };
    (root, dir, base, ext, name)
}

/// `true` iff `c` is an ASCII letter — Node's `isWindowsDeviceRoot` (a
/// drive-letter check for `toNamespacedPath`).
#[cfg(target_os = "windows")]
fn is_windows_device_root(c: u8) -> bool {
    c.is_ascii_alphabetic()
}

/// `path.toNamespacedPath(path)` on Windows: resolves `path` to an absolute
/// form, then prefixes a UNC root (`\\server\share\...`) with `\\?\UNC\` or a
/// drive root (`C:\...`) with `\\?\`, producing the long-path-escaped form
/// Windows Win32 file APIs accept unmodified. Mirrors Node's
/// `path.win32.toNamespacedPath`. Non-qualifying/too-short results (already a
/// `\\?\...` or `\\.\...` device path, or a resolved path of length <= 2) are
/// returned unchanged, same as Node.
#[cfg(target_os = "windows")]
fn to_namespaced_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let resolved = resolve2(path, "");
    if resolved.len() <= 2 {
        return path.to_string();
    }
    let bytes = resolved.as_bytes();
    if bytes[0] == b'\\' && bytes[1] == b'\\' {
        let third = bytes[2];
        if third != b'?' && third != b'.' {
            return format!("\\\\?\\UNC\\{}", &resolved[2..]);
        }
    } else if is_windows_device_root(bytes[0]) && bytes[1] == b':' && bytes[2] == b'\\' {
        return format!("\\\\?\\{resolved}");
    }
    path.to_string()
}

/// `path.toNamespacedPath(path)` on non-Windows targets: a documented no-op
/// (Node: "this method is meaningful only on Windows systems... on POSIX
/// systems the method is non-operational").
#[cfg(not(target_os = "windows"))]
fn to_namespaced_path(path: &str) -> String {
    path.to_string()
}

/// `path.join(a, b)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_JOIN(
    a_ptr: *const u8,
    a_len: i64,
    b_ptr: *const u8,
    b_len: i64,
) -> u64 {
    let (Some(a), Some(b)) =
        (unsafe { from_abi(a_ptr, a_len) }, unsafe { from_abi(b_ptr, b_len) })
    else {
        return 0;
    };
    intern(&join2(a, b))
}

/// `path.resolve(a, b)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_RESOLVE(
    a_ptr: *const u8,
    a_len: i64,
    b_ptr: *const u8,
    b_len: i64,
) -> u64 {
    let (Some(a), Some(b)) =
        (unsafe { from_abi(a_ptr, a_len) }, unsafe { from_abi(b_ptr, b_len) })
    else {
        return 0;
    };
    intern(&resolve2(a, b))
}

/// `path.normalize(path)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_NORMALIZE(ptr: *const u8, len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&normalize_str(path))
}

/// `path.dirname(path)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_DIRNAME(ptr: *const u8, len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&dirname(path))
}

/// `path.basename(path)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_BASENAME(ptr: *const u8, len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&basename(path))
}

/// `path.basename(path, suffix)` (fixed-arity split — see module doc).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_BASENAME_EXT(
    path_ptr: *const u8,
    path_len: i64,
    suffix_ptr: *const u8,
    suffix_len: i64,
) -> u64 {
    let (Some(path), Some(suffix)) = (
        unsafe { from_abi(path_ptr, path_len) },
        unsafe { from_abi(suffix_ptr, suffix_len) },
    ) else {
        return 0;
    };
    intern(&basename_ext(path, suffix))
}

/// `path.extname(path)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_EXTNAME(ptr: *const u8, len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&extname(path))
}

/// `path.isAbsolute(path)`. Node's own `isAbsolute` (both posix and win32)
/// treats a leading path separator as sufficient — a Windows drive letter is
/// NOT required (`path.win32.isAbsolute('/foo') === true`) — which is
/// exactly `Path::has_root()`, not `Path::is_absolute()` (the latter also
/// demands a Windows drive/UNC prefix and would wrongly say `false` here).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_IS_ABSOLUTE(ptr: *const u8, len: i64) -> i64 {
    let Some(path) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    if Path::new(path).has_root() {
        1
    } else {
        0
    }
}

/// `path.relative(from, to)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_RELATIVE(
    from_ptr: *const u8,
    from_len: i64,
    to_ptr: *const u8,
    to_len: i64,
) -> u64 {
    let (Some(from), Some(to)) = (
        unsafe { from_abi(from_ptr, from_len) },
        unsafe { from_abi(to_ptr, to_len) },
    ) else {
        return 0;
    };
    intern(&relative(from, to))
}

/// `path.parse(path)` — returns `{ root, dir, base, ext, name }` as a real
/// shaped object (`alloc_shaped_object`), each field a string PolyValue word.
/// `0` (the same null-handle sentinel every other `Handle`-returning member in
/// this file uses) on invalid input (null pointer / non-UTF-8 bytes).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_PARSE(ptr: *const u8, len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    let (root, dir, base, ext, name) = parse_path(path);
    alloc_shaped_object(
        &["root", "dir", "base", "ext", "name"],
        &[
            string_word(root.as_bytes()) as i64,
            string_word(dir.as_bytes()) as i64,
            string_word(base.as_bytes()) as i64,
            string_word(ext.as_bytes()) as i64,
            string_word(name.as_bytes()) as i64,
        ],
    )
}

/// `path.toNamespacedPath(path)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_TO_NAMESPACED_PATH(ptr: *const u8, len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&to_namespaced_path(path))
}
