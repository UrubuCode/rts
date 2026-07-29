//! node:path — Win32 free-function entry points (non-variadic functions;
//! `join`/`resolve` overloads are generated in the parent module — they are
//! Node-variadic, which `#[rtse::function]` has no form for yet).

use super::super::flavor::{chars, Flavor};
use super::super::{classify, glob, parse as pmod, win32, words};
use super::{drive_cwd, process_cwd};

use rts_engine::abi::ty::Handle;

const F: Flavor = Flavor::Win32;

/// `path.win32.basename(path[, suffix])`.
#[rtse::function(module = "node:path/win32", value = "basename")]
fn basename(p: &str, suffix: Option<&str>) -> String {
    let suffix_chars = suffix.map(chars);
    classify::basename(&chars(p), suffix_chars.as_deref(), F)
}

/// `path.win32.dirname(path)`.
#[rtse::function(module = "node:path/win32", value = "dirname")]
fn dirname(p: &str) -> String {
    classify::dirname(&chars(p), F)
}

/// `path.win32.extname(path)`.
#[rtse::function(module = "node:path/win32", value = "extname")]
fn extname(p: &str) -> String {
    classify::extname(&chars(p), F)
}

/// `path.win32.isAbsolute(path)`.
#[rtse::function(module = "node:path/win32", value = "isAbsolute")]
fn is_absolute(p: &str) -> bool {
    classify::is_absolute(&chars(p), F)
}

/// `path.win32.normalize(path)`.
#[rtse::function(module = "node:path/win32", value = "normalize")]
fn normalize(p: &str) -> String {
    win32::normalize(p)
}

/// `path.win32.relative(from, to)`.
#[rtse::function(module = "node:path/win32", value = "relative")]
fn relative(from: &str, to: &str) -> String {
    win32::relative(from, to, &process_cwd(), drive_cwd)
}

/// `path.win32.parse(path)`.
#[rtse::function(module = "node:path/win32", value = "parse")]
fn parse(p: &str) -> Handle {
    words::parsed_object(&pmod::win32_parse(p))
}

/// `path.win32.format(pathObject)`.
#[rtse::function(module = "node:path/win32", value = "format")]
fn format(path_object: Handle) -> String {
    let (root, dir, base, name, ext) = words::format_fields(path_object);
    pmod::format(root, dir, base, name, ext, F)
}

/// `path.win32.toNamespacedPath(path)`.
#[rtse::function(module = "node:path/win32", value = "toNamespacedPath")]
fn to_namespaced_path(p: &str) -> String {
    let resolved = win32::resolve(&[p.to_string()], &process_cwd(), drive_cwd);
    win32::to_namespaced(p, &resolved)
}

/// `path.win32.matchesGlob(path, pattern)`.
#[rtse::function(module = "node:path/win32", value = "matchesGlob")]
fn matches_glob(p: &str, pattern: &str) -> bool {
    glob::matches_glob(p, pattern, F)
}
