//! node:fs — `globSync(pattern)`: filesystem glob matching via the pure-Rust
//! `glob` crate (`*`, `**`, `?`, `[...]`). Returns the matching paths as a
//! `string[]`, in the crate's sorted order. Real directory traversal, no
//! fabricated results.

use rts_engine::abi::ty::Handle;

use super::words::string_array;

/// `fs.globSync(pattern)` → `string[]` of paths matching the glob `pattern`
/// (relative to the current working directory unless the pattern is absolute).
/// An invalid pattern yields an empty list rather than throwing (the malformed
/// pattern simply matches nothing).
///
/// Authored with `#[rtse::function]`; `fs/mod.rs` patches `THROWS` on at
/// registration (matching the previous `func(...)` row, though this body never
/// actually throws — an invalid pattern degrades to an empty list).
#[rtse::function(module = "node:fs", value = "globSync")]
fn glob_sync(pattern: &str) -> Handle {
    let mut names: Vec<String> = Vec::new();
    if let Ok(paths) = glob::glob(pattern) {
        for path in paths.flatten() {
            names.push(path.to_string_lossy().into_owned());
        }
    }
    string_array(&names)
}
