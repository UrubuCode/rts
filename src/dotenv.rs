//! Minimal `.env` loader. Reads `<project_root>/.env` and injects into the
//! process environment before executing user programs. Variables already set
//! in the environment are NOT overwritten (same behaviour as `dotenv` crate).

use std::path::Path;

/// Load `.env` from `dir` (if it exists) into the process environment.
/// Silently skips if the file is absent or unreadable.
pub fn load_from_dir(dir: &Path) {
    let path = dir.join(".env");
    if !path.exists() {
        return;
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = unquote(value.trim());
        if key.is_empty() {
            continue;
        }
        // Only set if not already present
        if std::env::var(key).is_err() {
            unsafe { std::env::set_var(key, value) };
        }
    }
}

fn unquote(s: &str) -> &str {
    if s.len() >= 2 {
        if (s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\''))
        {
            return &s[1..s.len() - 1];
        }
    }
    s
}
