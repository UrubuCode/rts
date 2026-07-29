//! node:path — POSIX free-function entry points (the non-variadic functions;
//! `join`/`resolve` overloads are generated in the parent module — they are
//! Node-variadic, which `#[rtse::function]` has no form for yet).

use super::super::flavor::{chars, Flavor};
use super::super::{classify, glob, parse as pmod, posix, words};
use super::process_cwd;

use rts_engine::abi::ty::Handle;

const F: Flavor = Flavor::Posix;

/// `path.posix.basename(path[, suffix])`.
#[rtse::function(module = "node:path/posix", value = "basename")]
fn basename(p: &str, suffix: Option<&str>) -> String {
    let suffix_chars = suffix.map(chars);
    classify::basename(&chars(p), suffix_chars.as_deref(), F)
}

/// `path.posix.dirname(path)`.
#[rtse::function(module = "node:path/posix", value = "dirname")]
fn dirname(p: &str) -> String {
    classify::dirname(&chars(p), F)
}

/// `path.posix.extname(path)`.
#[rtse::function(module = "node:path/posix", value = "extname")]
fn extname(p: &str) -> String {
    classify::extname(&chars(p), F)
}

/// `path.posix.isAbsolute(path)`.
#[rtse::function(module = "node:path/posix", value = "isAbsolute")]
fn is_absolute(p: &str) -> bool {
    classify::is_absolute(&chars(p), F)
}

/// `path.posix.normalize(path)`.
#[rtse::function(module = "node:path/posix", value = "normalize")]
fn normalize(p: &str) -> String {
    posix::normalize(p)
}

/// `path.posix.relative(from, to)`.
#[rtse::function(module = "node:path/posix", value = "relative")]
fn relative(from: &str, to: &str) -> String {
    posix::relative(from, to, &process_cwd())
}

/// `path.posix.parse(path)`.
#[rtse::function(module = "node:path/posix", value = "parse")]
fn parse(p: &str) -> Handle {
    words::parsed_object(&posix::parse(p))
}

/// `path.posix.format(pathObject)`.
#[rtse::function(module = "node:path/posix", value = "format")]
fn format(path_object: Handle) -> String {
    let (root, dir, base, name, ext) = words::format_fields(path_object);
    pmod::format(root, dir, base, name, ext, F)
}

/// `path.posix.toNamespacedPath(path)`.
#[rtse::function(module = "node:path/posix", value = "toNamespacedPath")]
fn to_namespaced_path(p: &str) -> String {
    posix::to_namespaced(p)
}

/// `path.posix.matchesGlob(path, pattern)`.
#[rtse::function(module = "node:path/posix", value = "matchesGlob")]
fn matches_glob(p: &str, pattern: &str) -> bool {
    glob::matches_glob(p, pattern, F)
}
