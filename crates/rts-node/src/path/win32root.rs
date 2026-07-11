//! node:path — the Win32 ROOT scanner shared by `normalize`/`resolve`/`parse`/
//! `dirname`/`isAbsolute`/`toNamespacedPath`. Faithful port of the rootEnd/
//! device/isAbsolute computation at the top of Node's `win32.normalize`/
//! `win32.parse` — the single source of Windows drive-letter and UNC parsing.

use super::flavor::{is_letter, Flavor};

const F: Flavor = Flavor::Win32;

/// The parsed Windows root of a path.
pub struct WinRoot {
    /// `Some("C:")` for a drive, `Some("\\\\server\\share")` for UNC, `None`
    /// for a bare-rooted (`\foo`) or relative path.
    pub device: Option<String>,
    /// Number of leading chars that form the root prefix (what `parse().root`
    /// slices, and where the normalizable tail begins).
    pub root_end: usize,
    pub is_absolute: bool,
}

/// Scan the leading root of a Win32 path.
pub fn win_root(path: &[char]) -> WinRoot {
    let len = path.len();
    if len == 0 {
        return WinRoot { device: None, root_end: 0, is_absolute: false };
    }
    let code = path[0];

    if F.is_sep(code) {
        // Absolute path beginning with a separator.
        if len > 1 && F.is_sep(path[1]) {
            // Possible UNC root: \\server\share
            let mut j = 2usize;
            let mut last = j;
            while j < len && !F.is_sep(path[j]) {
                j += 1;
            }
            if j < len && j != last {
                // server = path[last..j]
                last = j;
                while j < len && F.is_sep(path[j]) {
                    j += 1;
                }
                if j < len && j != last {
                    last = j;
                    while j < len && !F.is_sep(path[j]) {
                        j += 1;
                    }
                    // share = path[last..j]; root spans to j (or len)
                    let root_end = j;
                    let device: String = path[..root_end].iter().collect();
                    return WinRoot { device: Some(device), root_end, is_absolute: true };
                }
            }
        }
        // Bare rooted path: "\foo".
        return WinRoot { device: None, root_end: 1, is_absolute: true };
    }

    if is_letter(code) && len > 1 && path[1] == ':' {
        let device: String = path[..2].iter().collect();
        if len > 2 && F.is_sep(path[2]) {
            return WinRoot { device: Some(device), root_end: 3, is_absolute: true };
        }
        // Drive-relative: "C:" / "C:foo".
        return WinRoot { device: Some(device), root_end: 2, is_absolute: false };
    }

    WinRoot { device: None, root_end: 0, is_absolute: false }
}
