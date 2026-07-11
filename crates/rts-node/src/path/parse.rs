//! node:path — `parse` (→ components) and `format` (components →), both flavors.
//! Faithful ports of Node's `posix.parse`/`win32.parse`/`_format`.

use super::flavor::{is_letter, Flavor};
use super::win32root::win_root;

/// `{ root, dir, base, ext, name }`.
#[derive(Default)]
pub struct Parsed {
    pub root: String,
    pub dir: String,
    pub base: String,
    pub ext: String,
    pub name: String,
}

/// `path.posix.parse(path)`.
pub fn posix_parse(path: &str) -> Parsed {
    let c: Vec<char> = path.chars().collect();
    let mut ret = Parsed::default();
    if c.is_empty() {
        return ret;
    }
    let is_absolute = c[0] == '/';
    let start = if is_absolute {
        ret.root = "/".to_string();
        1
    } else {
        0
    };
    fill_base_dir(&c, start, is_absolute, "/".chars().collect::<Vec<_>>().as_slice(), &mut ret, Flavor::Posix);
    ret
}

/// `path.win32.parse(path)`.
pub fn win32_parse(path: &str) -> Parsed {
    let c: Vec<char> = path.chars().collect();
    let mut ret = Parsed::default();
    if c.is_empty() {
        return ret;
    }
    let root = win_root(&c);
    let start = root.root_end;
    if root.root_end > 0 {
        ret.root = c[..root.root_end].iter().collect();
    }
    let root_chars: Vec<char> = ret.root.chars().collect();
    fill_base_dir(&c, start, root.is_absolute, &root_chars, &mut ret, Flavor::Win32);
    ret
}

/// Shared component extraction over `path[start..]`, filling base/ext/name/dir.
fn fill_base_dir(
    c: &[char],
    start: usize,
    is_absolute: bool,
    root: &[char],
    ret: &mut Parsed,
    flavor: Flavor,
) {
    let len = c.len();
    let mut start_dot: isize = -1;
    let mut start_part = start;
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut pre_dot_state: isize = 0;
    let mut i = len as isize - 1;
    while i >= start as isize {
        let code = c[i as usize];
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

    if end != -1 {
        let sp = start_part;
        if start_dot == -1
            || pre_dot_state == 0
            || (pre_dot_state == 1 && start_dot == end - 1 && start_dot == sp as isize + 1)
        {
            ret.base = slice(c, sp, end as usize);
            ret.name = ret.base.clone();
        } else {
            ret.name = slice(c, sp, start_dot as usize);
            ret.base = slice(c, sp, end as usize);
            ret.ext = slice(c, start_dot as usize, end as usize);
        }
    }

    if start_part > 0 {
        ret.dir = slice(c, 0, start_part - 1);
    } else if is_absolute {
        ret.dir = root.iter().collect();
    }
    let _ = is_letter; // referenced for parity symmetry with win_root callers
}

fn slice(c: &[char], start: usize, end: usize) -> String {
    if start >= end || start >= c.len() {
        return String::new();
    }
    c[start..end.min(c.len())].iter().collect()
}

/// `path.<flavor>.format(pathObject)`.
pub fn format(
    root: Option<String>,
    dir: Option<String>,
    base: Option<String>,
    name: Option<String>,
    ext: Option<String>,
    flavor: Flavor,
) -> String {
    let root_s = root.unwrap_or_default();
    let dir_s = dir.filter(|s| !s.is_empty()).unwrap_or_else(|| root_s.clone());
    let base_s = base.filter(|s| !s.is_empty()).unwrap_or_else(|| {
        let n = name.unwrap_or_default();
        let e = ext.unwrap_or_default();
        format!("{n}{}", ext_with_dot(&e))
    });
    if dir_s.is_empty() {
        return base_s;
    }
    if dir_s == root_s {
        return format!("{dir_s}{base_s}");
    }
    format!("{dir_s}{}{base_s}", flavor.sep())
}

/// Ensure a leading `.` on a non-empty extension (Node ≥19 auto-insert).
fn ext_with_dot(ext: &str) -> String {
    if ext.is_empty() || ext.starts_with('.') {
        ext.to_string()
    } else {
        format!(".{ext}")
    }
}
