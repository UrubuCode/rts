//! node:path — component classification/extraction: `isAbsolute`, `basename`,
//! `dirname`, `extname`. Faithful ports of Node's `lib/path.js` (both flavors).

use super::flavor::{is_letter, Flavor};

/// `isAbsolute(path)`.
pub fn is_absolute(path: &[char], flavor: Flavor) -> bool {
    let len = path.len();
    if len == 0 {
        return false;
    }
    let c0 = path[0];
    if flavor.is_sep(c0) {
        return true;
    }
    if flavor == Flavor::Win32 && is_letter(c0) && len > 2 && path[1] == ':' && flavor.is_sep(path[2]) {
        return true;
    }
    false
}

/// `basename(path[, suffix])`.
pub fn basename(path: &[char], suffix: Option<&[char]>, flavor: Flavor) -> String {
    // Win32 skips a leading drive designator ("C:").
    let mut start = drive_skip(path, flavor);
    let mut end: isize = -1;
    let mut matched_slash = true;

    if let Some(suf) = suffix {
        if !suf.is_empty() && suf.len() <= path.len() {
            if suf == path {
                return String::new();
            }
            let mut ext_idx: isize = suf.len() as isize - 1;
            let mut first_non_slash_end: isize = -1;
            let mut i = path.len() as isize - 1;
            while i >= start as isize {
                let code = path[i as usize];
                if flavor.is_sep(code) {
                    if !matched_slash {
                        start = (i + 1) as usize;
                        break;
                    }
                } else {
                    if first_non_slash_end == -1 {
                        matched_slash = false;
                        first_non_slash_end = i + 1;
                    }
                    if ext_idx >= 0 {
                        if code == suf[ext_idx as usize] {
                            ext_idx -= 1;
                            if ext_idx == -1 {
                                end = i;
                            }
                        } else {
                            ext_idx = -1;
                            end = first_non_slash_end;
                        }
                    }
                }
                i -= 1;
            }
            if start as isize == end {
                end = first_non_slash_end;
            } else if end == -1 {
                end = path.len() as isize;
            }
            return slice(path, start, end.max(start as isize) as usize);
        }
    }

    let mut i = path.len() as isize - 1;
    while i >= start as isize {
        if flavor.is_sep(path[i as usize]) {
            if !matched_slash {
                start = (i + 1) as usize;
                break;
            }
        } else if end == -1 {
            matched_slash = false;
            end = i + 1;
        }
        i -= 1;
    }
    if end == -1 {
        return String::new();
    }
    slice(path, start, end as usize)
}

/// `dirname(path)`.
pub fn dirname(path: &[char], flavor: Flavor) -> String {
    let len = path.len();
    if len == 0 {
        return ".".to_string();
    }
    let (root_end, has_root) = win_root_end(path, flavor);
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut i = len as isize - 1;
    while i >= root_end as isize {
        if flavor.is_sep(path[i as usize]) {
            if !matched_slash {
                end = i;
                break;
            }
        } else {
            matched_slash = false;
        }
        i -= 1;
    }

    if end == -1 {
        if has_root {
            return slice(path, 0, root_end);
        }
        if root_end > 0 {
            // Win32 drive-relative like "C:foo" → "C:".
            return slice(path, 0, root_end);
        }
        return if flavor.is_sep(path[0]) {
            flavor.sep().to_string()
        } else {
            ".".to_string()
        };
    }
    if end == 0 && flavor.is_sep(path[0]) {
        return flavor.sep().to_string();
    }
    if (end as usize) < root_end {
        return slice(path, 0, root_end);
    }
    slice(path, 0, end as usize)
}

/// `extname(path)`.
pub fn extname(path: &[char], flavor: Flavor) -> String {
    let start = drive_skip(path, flavor);
    let mut start_dot: isize = -1;
    let mut start_part = start;
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut pre_dot_state: isize = 0;

    let mut i = path.len() as isize - 1;
    while i >= start as isize {
        let code = path[i as usize];
        if flavor.is_sep(code) {
            if !matched_slash {
                start_part = (i + 1) as usize;
                break;
            }
            i -= 1;
            continue;
        }
        if end == -1 {
            matched_slash = false;
            end = i + 1;
        }
        if code == '.' {
            if start_dot == -1 {
                start_dot = i;
            } else if pre_dot_state != 1 {
                pre_dot_state = 1;
            }
        } else if start_dot != -1 {
            pre_dot_state = -1;
        }
        i -= 1;
    }

    if start_dot == -1
        || end == -1
        || pre_dot_state == 0
        || (pre_dot_state == 1 && start_dot == end - 1 && start_dot == start_part as isize + 1)
    {
        return String::new();
    }
    slice(path, start_dot as usize, end as usize)
}

/// Win32 leading drive designator length (`0` on POSIX / no drive).
fn drive_skip(path: &[char], flavor: Flavor) -> usize {
    if flavor == Flavor::Win32 && path.len() >= 2 && is_letter(path[0]) && path[1] == ':' {
        2
    } else {
        0
    }
}

/// The end index of the ROOT (`(root_end, has_absolute_root)`) — how many
/// leading chars are the root prefix, and whether the path is rooted.
fn win_root_end(path: &[char], flavor: Flavor) -> (usize, bool) {
    if flavor == Flavor::Posix {
        return if !path.is_empty() && path[0] == '/' {
            (1, true)
        } else {
            (0, false)
        };
    }
    // Win32.
    let len = path.len();
    if len == 0 {
        return (0, false);
    }
    if flavor.is_sep(path[0]) {
        // UNC or root-relative: root is the leading separator run (Node treats
        // "\\server\share" specially, but dirname only needs the rooted flag).
        return (1, true);
    }
    if len >= 2 && is_letter(path[0]) && path[1] == ':' {
        if len > 2 && flavor.is_sep(path[2]) {
            return (3, true); // C:\
        }
        return (2, false); // C: (drive-relative)
    }
    (0, false)
}

fn slice(path: &[char], start: usize, end: usize) -> String {
    if start >= end || start >= path.len() {
        return String::new();
    }
    path[start..end.min(path.len())].iter().collect()
}
