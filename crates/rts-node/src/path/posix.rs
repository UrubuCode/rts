//! node:path — the POSIX flavor (`lib/path.js` `posix` object), full port.
//! `basename`/`dirname`/`extname`/`isAbsolute` come from the shared
//! [`super::classify`]; the root-aware functions live here.

use super::flavor::{chars, normalize_string, Flavor};

const F: Flavor = Flavor::Posix;

/// `path.posix.normalize(path)`.
pub fn normalize(path: &str) -> String {
    let c = chars(path);
    if c.is_empty() {
        return ".".to_string();
    }
    let is_absolute = c[0] == '/';
    let trailing_sep = *c.last().unwrap() == '/';
    let mut s = normalize_string(&c, !is_absolute, F);
    if s.is_empty() {
        if is_absolute {
            return "/".to_string();
        }
        return if trailing_sep { "./".to_string() } else { ".".to_string() };
    }
    if trailing_sep {
        s.push('/');
    }
    if is_absolute {
        format!("/{s}")
    } else {
        s
    }
}

/// `path.posix.join(...parts)`.
pub fn join(parts: &[String]) -> String {
    if parts.is_empty() {
        return ".".to_string();
    }
    let joined = parts
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("/");
    if joined.is_empty() {
        return ".".to_string();
    }
    normalize(&joined)
}

/// `path.posix.resolve(...parts)` — `cwd` supplies the process directory.
pub fn resolve(parts: &[String], cwd: &str) -> String {
    let mut resolved: Vec<char> = Vec::new();
    let mut absolute = false;
    let mut i = parts.len() as isize - 1;
    while i >= -1 && !absolute {
        let seg = if i >= 0 {
            parts[i as usize].clone()
        } else {
            cwd.to_string()
        };
        i -= 1;
        if seg.is_empty() {
            continue;
        }
        let mut next: Vec<char> = seg.chars().collect();
        next.push('/');
        next.extend_from_slice(&resolved);
        resolved = next;
        absolute = seg.starts_with('/');
    }
    let norm = normalize_string(&resolved, !absolute, F);
    if absolute {
        format!("/{norm}")
    } else if norm.is_empty() {
        ".".to_string()
    } else {
        norm
    }
}

/// `path.posix.relative(from, to)`.
pub fn relative(from: &str, to: &str, cwd: &str) -> String {
    if from == to {
        return String::new();
    }
    let f = resolve(&[from.to_string()], cwd);
    let t = resolve(&[to.to_string()], cwd);
    if f == t {
        return String::new();
    }
    super::relative::posix_relative(&f, &t)
}

/// `path.posix.parse(path)` → `(root, dir, base, ext, name)`.
pub fn parse(path: &str) -> super::parse::Parsed {
    super::parse::posix_parse(path)
}

/// `path.posix.toNamespacedPath(path)` — no-op on POSIX.
pub fn to_namespaced(path: &str) -> String {
    path.to_string()
}
