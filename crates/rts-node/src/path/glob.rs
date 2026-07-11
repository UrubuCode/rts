//! node:path — `matchesGlob(path, pattern)`. A separator-aware glob matcher
//! (`*` within a segment, `**` across segments, `?` single non-separator,
//! `[set]` char class). Separator set is flavor-specific (`/` on POSIX; `/`
//! and `\` on Win32).

use super::flavor::Flavor;

/// Whether `path` matches glob `pattern` under `flavor`'s separator rules.
pub fn matches_glob(path: &str, pattern: &str, flavor: Flavor) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    matches(&p, 0, &s, 0, flavor)
}

fn is_sep(c: char, flavor: Flavor) -> bool {
    flavor.is_sep(c)
}

fn matches(p: &[char], pi: usize, s: &[char], si: usize, flavor: Flavor) -> bool {
    let mut pi = pi;
    let mut si = si;
    while pi < p.len() {
        match p[pi] {
            '*' => {
                let double = pi + 1 < p.len() && p[pi + 1] == '*';
                let next_pi = if double { pi + 2 } else { pi + 1 };
                // Try to match the rest against every possible advance of `s`.
                if matches(p, next_pi, s, si, flavor) {
                    return true;
                }
                let mut k = si;
                while k < s.len() {
                    // `*` does not cross a separator; `**` does.
                    if !double && is_sep(s[k], flavor) {
                        break;
                    }
                    k += 1;
                    if matches(p, next_pi, s, k, flavor) {
                        return true;
                    }
                }
                return false;
            }
            '?' => {
                if si >= s.len() || is_sep(s[si], flavor) {
                    return false;
                }
                pi += 1;
                si += 1;
            }
            '[' => {
                if si >= s.len() {
                    return false;
                }
                match match_class(p, pi, s[si]) {
                    Some(after) => {
                        pi = after;
                        si += 1;
                    }
                    None => return false,
                }
            }
            c => {
                if si >= s.len() {
                    return false;
                }
                let sc = s[si];
                let eq = c == sc || (is_sep(c, flavor) && is_sep(sc, flavor));
                if !eq {
                    return false;
                }
                pi += 1;
                si += 1;
            }
        }
    }
    si == s.len()
}

/// Match a `[...]` class at `p[start]` against `c`; returns the index just
/// after the class on success.
fn match_class(p: &[char], start: usize, c: char) -> Option<usize> {
    let mut i = start + 1;
    let negate = i < p.len() && (p[i] == '!' || p[i] == '^');
    if negate {
        i += 1;
    }
    let mut matched = false;
    let mut first = true;
    while i < p.len() && (p[i] != ']' || first) {
        first = false;
        // Range a-z.
        if i + 2 < p.len() && p[i + 1] == '-' && p[i + 2] != ']' {
            if c >= p[i] && c <= p[i + 2] {
                matched = true;
            }
            i += 3;
        } else {
            if p[i] == c {
                matched = true;
            }
            i += 1;
        }
    }
    if i >= p.len() {
        return None; // unterminated class
    }
    // skip ']'
    let after = i + 1;
    if matched != negate {
        Some(after)
    } else {
        None
    }
}
