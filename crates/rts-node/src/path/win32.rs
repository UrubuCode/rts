//! node:path — the Win32 flavor (`lib/path.js` `win32` object), full port.
//! Drive-letter + UNC aware; accepts `/` and `\` on input, emits `\`.

use super::flavor::{chars, is_letter, normalize_string, Flavor};
use super::win32root::win_root;

const F: Flavor = Flavor::Win32;

/// `path.win32.normalize(path)`.
pub fn normalize(path: &str) -> String {
    let c = chars(path);
    let len = c.len();
    if len == 0 {
        return ".".to_string();
    }
    let root = win_root(&c);
    let trailing_sep = F.is_sep(c[len - 1]);

    let mut tail = if root.root_end < len {
        normalize_string(&c[root.root_end..], !root.is_absolute, F)
    } else {
        String::new()
    };
    if tail.is_empty() && !root.is_absolute {
        tail = ".".to_string();
    }
    if !tail.is_empty() && trailing_sep {
        tail.push('\\');
    }
    match root.device {
        None => {
            if root.is_absolute {
                format!("\\{tail}")
            } else {
                tail
            }
        }
        Some(dev) => {
            if root.is_absolute {
                format!("{dev}\\{tail}")
            } else {
                format!("{dev}{tail}")
            }
        }
    }
}

/// `path.win32.join(...parts)`.
pub fn join(parts: &[String]) -> String {
    if parts.is_empty() {
        return ".".to_string();
    }
    let mut joined: Option<String> = None;
    let mut first_part = String::new();
    for arg in parts.iter().filter(|p| !p.is_empty()) {
        match &mut joined {
            None => {
                first_part = arg.clone();
                joined = Some(arg.clone());
            }
            Some(j) => {
                j.push('\\');
                j.push_str(arg);
            }
        }
    }
    let mut joined = match joined {
        Some(j) => j,
        None => return ".".to_string(),
    };

    // UNC leading-separator fixup (Node's win32.join).
    let fp: Vec<char> = first_part.chars().collect();
    let mut needs_replace = true;
    let mut slash_count = 0usize;
    if !fp.is_empty() && F.is_sep(fp[0]) {
        slash_count += 1;
        if fp.len() > 1 && F.is_sep(fp[1]) {
            slash_count += 1;
            if fp.len() > 2 {
                if F.is_sep(fp[2]) {
                    slash_count += 1;
                } else {
                    needs_replace = false;
                }
            }
        }
    }
    if needs_replace {
        let jc: Vec<char> = joined.chars().collect();
        while slash_count < jc.len() && F.is_sep(jc[slash_count]) {
            slash_count += 1;
        }
        if slash_count >= 2 {
            joined = format!("\\{}", jc[slash_count..].iter().collect::<String>());
        }
    }
    normalize(&joined)
}

/// `path.win32.resolve(...parts)`. `process_cwd` is the process directory;
/// `drive_cwd(letter)` supplies a drive-specific working directory.
pub fn resolve(parts: &[String], process_cwd: &str, drive_cwd: impl Fn(char) -> String) -> String {
    let mut resolved_device = String::new();
    let mut resolved_tail: Vec<char> = Vec::new();
    let mut resolved_absolute = false;

    let mut i = parts.len() as isize - 1;
    while i >= -1 && !resolved_absolute {
        let seg: String = if i >= 0 {
            parts[i as usize].clone()
        } else if resolved_device.is_empty() {
            process_cwd.to_string()
        } else {
            // A path for a specific drive letter — its own working directory.
            let letter = resolved_device.chars().next().unwrap_or('C');
            let dc = drive_cwd(letter);
            // If it doesn't already start with the device, prefix it.
            if dc.to_lowercase().starts_with(&resolved_device.to_lowercase()) {
                dc
            } else {
                format!("{resolved_device}\\{dc}")
            }
        };
        i -= 1;
        if seg.is_empty() {
            continue;
        }
        let sc = chars(&seg);
        let root = win_root(&sc);
        let device = root.device.clone().unwrap_or_default();

        if !device.is_empty() {
            if !resolved_device.is_empty() {
                if device.to_lowercase() != resolved_device.to_lowercase() {
                    continue;
                }
            } else {
                resolved_device = device;
            }
        }

        if resolved_absolute {
            if !resolved_device.is_empty() {
                break;
            }
        } else {
            let tail: Vec<char> = sc[root.root_end..].to_vec();
            let mut next = tail;
            next.push('\\');
            next.extend_from_slice(&resolved_tail);
            resolved_tail = next;
            resolved_absolute = root.is_absolute;
            if root.is_absolute && !resolved_device.is_empty() {
                break;
            }
        }
    }

    let norm = normalize_string(&resolved_tail, !resolved_absolute, F);
    let out = format!(
        "{resolved_device}{}{norm}",
        if resolved_absolute { "\\" } else { "" }
    );
    if out.is_empty() {
        ".".to_string()
    } else {
        out
    }
}

/// `path.win32.relative(from, to)`.
pub fn relative(from: &str, to: &str, process_cwd: &str, drive_cwd: impl Fn(char) -> String + Copy) -> String {
    if from == to {
        return String::new();
    }
    let f = resolve(&[from.to_string()], process_cwd, drive_cwd);
    let t = resolve(&[to.to_string()], process_cwd, drive_cwd);
    if f.to_lowercase() == t.to_lowercase() {
        return String::new();
    }
    super::relative::win32_relative(&f, &t)
}

/// `path.win32.toNamespacedPath(path)` — `\\?\` / `\\?\UNC\` long-path form.
pub fn to_namespaced(path: &str, resolved: &str) -> String {
    if path.is_empty() {
        return path.to_string();
    }
    let rc = chars(resolved);
    if rc.len() < 3 {
        return path.to_string();
    }
    if F.is_sep(rc[0]) && F.is_sep(rc[1]) {
        // UNC: \\server\... → \\?\UNC\server\...
        if rc.len() >= 3 && rc[2] != '?' && rc[2] != '.' {
            let rest: String = rc[2..].iter().collect();
            return format!("\\\\?\\UNC\\{rest}");
        }
    } else if is_letter(rc[0]) && rc[1] == ':' && F.is_sep(rc[2]) {
        return format!("\\\\?\\{resolved}");
    }
    path.to_string()
}
