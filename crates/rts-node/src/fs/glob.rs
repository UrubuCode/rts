//! node:fs — `globSync(pattern)`: filesystem glob matching via the pure-Rust
//! `glob` crate (`*`, `**`, `?`, `[...]`). Returns the matching paths as a
//! `string[]`, in the crate's sorted order. Real directory traversal, no
//! fabricated results.

use super::words::{read, string_array};

/// `fs.globSync(pattern)` → `string[]` of paths matching the glob `pattern`
/// (relative to the current working directory unless the pattern is absolute).
/// An invalid pattern yields an empty list rather than throwing (the malformed
/// pattern simply matches nothing).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_GLOB(p: *const u8, l: i64) -> u64 {
    let pattern = read(p, l);
    let mut names: Vec<String> = Vec::new();
    if let Ok(paths) = glob::glob(&pattern) {
        for path in paths.flatten() {
            names.push(path.to_string_lossy().into_owned());
        }
    }
    string_array(&names)
}
