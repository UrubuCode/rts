//! node:path — `relative(from, to)` common-prefix cores (both already resolved
//! to absolute paths by the caller). Faithful ports of Node's
//! `posix.relative`/`win32.relative`.

/// POSIX `relative` over two resolved absolute paths.
pub fn posix_relative(from: &str, to: &str) -> String {
    let f: Vec<char> = from.chars().collect();
    let t: Vec<char> = to.chars().collect();
    let from_start = 1;
    let from_end = f.len();
    let from_len = from_end - from_start;
    let to_start = 1;
    let to_end = t.len();
    let to_len = to_end - to_start;
    common_relative(&f, from_start, from_end, from_len, &t, to_start, to_end, to_len, '/')
}

/// Win32 `relative` over two resolved absolute paths (case-insensitive compare,
/// original-case output).
pub fn win32_relative(from_orig: &str, to_orig: &str) -> String {
    let fo: Vec<char> = from_orig.chars().collect();
    let to: Vec<char> = to_orig.chars().collect();
    let from_l: Vec<char> = from_orig.to_lowercase().chars().collect();
    let to_l: Vec<char> = to_orig.to_lowercase().chars().collect();

    let mut from_start = 0;
    while from_start < from_l.len() && from_l[from_start] == '\\' {
        from_start += 1;
    }
    let mut from_end = from_l.len();
    while from_end - 1 > from_start && from_l[from_end - 1] == '\\' {
        from_end -= 1;
    }
    let from_len = from_end - from_start;
    let mut to_start = 0;
    while to_start < to_l.len() && to_l[to_start] == '\\' {
        to_start += 1;
    }
    let mut to_end = to_l.len();
    while to_end - 1 > to_start && to_l[to_end - 1] == '\\' {
        to_end -= 1;
    }
    let to_len = to_end - to_start;

    let length = from_len.min(to_len);
    let mut last_common_sep: isize = -1;
    let mut i = 0usize;
    while i < length {
        let fc = from_l[from_start + i];
        if fc != to_l[to_start + i] {
            break;
        }
        if fc == '\\' {
            last_common_sep = i as isize;
        }
        i += 1;
    }
    if i != length {
        if last_common_sep == -1 {
            return to_orig.to_string();
        }
    } else {
        if to_len > length {
            if to_l[to_start + i] == '\\' {
                return to[to_start + i + 1..to_end].iter().collect();
            }
            if i == 2 {
                return to[to_start + i..to_end].iter().collect();
            }
        }
        if from_len > length {
            if from_l[from_start + i] == '\\' {
                last_common_sep = i as isize;
            } else if i == 2 {
                last_common_sep = i as isize;
            }
        }
        if last_common_sep == -1 {
            last_common_sep = 0;
        }
    }

    let mut out = String::new();
    let mut j = from_start as isize + last_common_sep + 1;
    while j <= from_end as isize {
        if j == from_end as isize || from_l[j as usize] == '\\' {
            out.push_str(if out.is_empty() { ".." } else { "\\.." });
        }
        j += 1;
    }
    let to_slice_start = to_start + last_common_sep as usize;
    if !out.is_empty() {
        let tail: String = to[to_slice_start..to_end].iter().collect();
        return format!("{out}{tail}");
    }
    let mut ts = to_slice_start;
    if ts < to.len() && to[ts] == '\\' {
        ts += 1;
    }
    to[ts..to_end].iter().collect()
}

#[allow(clippy::too_many_arguments)]
fn common_relative(
    f: &[char],
    from_start: usize,
    from_end: usize,
    from_len: usize,
    t: &[char],
    to_start: usize,
    to_end: usize,
    to_len: usize,
    sep: char,
) -> String {
    let length = from_len.min(to_len);
    let mut last_common_sep: isize = -1;
    let mut i = 0usize;
    while i < length {
        let fc = f[from_start + i];
        if fc != t[to_start + i] {
            break;
        }
        if fc == sep {
            last_common_sep = i as isize;
        }
        i += 1;
    }
    if i == length {
        if to_len > length {
            if t[to_start + i] == sep {
                return t[to_start + i + 1..to_end].iter().collect();
            }
            if i == 0 {
                return t[to_start + i..to_end].iter().collect();
            }
        } else if from_len > length {
            if f[from_start + i] == sep {
                last_common_sep = i as isize;
            } else if i == 0 {
                last_common_sep = 0;
            }
        }
    }

    let mut out = String::new();
    let mut j = from_start as isize + last_common_sep + 1;
    while j <= from_end as isize {
        if j == from_end as isize || f[j as usize] == sep {
            out.push_str(if out.is_empty() { ".." } else { "/.." });
        }
        j += 1;
    }
    let tail_start = to_start + (last_common_sep as usize);
    if !out.is_empty() {
        let tail: String = t[tail_start..to_end].iter().collect();
        format!("{out}{tail}")
    } else {
        let mut ts = tail_start;
        if ts < t.len() && t[ts] == sep {
            ts += 1;
        }
        t[ts..to_end].iter().collect()
    }
}
