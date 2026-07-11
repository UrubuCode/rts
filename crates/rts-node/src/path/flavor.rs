//! node:path — the OS-flavor core shared by every function.
//!
//! A faithful port of Node's own `lib/path.js` lexical algorithm (NOT
//! `std::path`, whose verbatim-prefix/trailing-dot/UNC handling diverges from
//! Node parity — see the module spec §5.1). Two flavors: POSIX (`/` only) and
//! Win32 (accepts `/` and `\` as input, emits `\`). Operates on `Vec<char>` so
//! non-ASCII filename bytes index correctly (separators/dots are all ASCII).

/// The path flavor a call operates in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Posix,
    Win32,
}

impl Flavor {
    /// The separator this flavor EMITS.
    pub fn sep(self) -> char {
        match self {
            Flavor::Posix => '/',
            Flavor::Win32 => '\\',
        }
    }

    /// The env `PATH` delimiter for this flavor.
    pub fn delimiter(self) -> char {
        match self {
            Flavor::Posix => ':',
            Flavor::Win32 => ';',
        }
    }

    /// Whether `c` is accepted as a separator on INPUT (`\` and `/` on Win32).
    pub fn is_sep(self, c: char) -> bool {
        match self {
            Flavor::Posix => c == '/',
            Flavor::Win32 => c == '/' || c == '\\',
        }
    }
}

/// `c` is an ASCII drive letter (`A..Z`/`a..z`).
pub fn is_letter(c: char) -> bool {
    c.is_ascii_alphabetic()
}

/// Node's `normalizeString`: resolve `.`/`..` segments and collapse separators
/// over the slice `s`, emitting `sep`, allowing `..` above the root only when
/// `allow_above_root`. Pure lexical — never touches the filesystem.
pub fn normalize_string(s: &[char], allow_above_root: bool, flavor: Flavor) -> String {
    let sep = flavor.sep();
    let mut res: Vec<char> = Vec::new();
    let mut last_segment_length = 0usize;
    let mut last_slash: isize = -1;
    let mut dots: isize = 0;
    let len = s.len();

    for i in 0..=len {
        let ch = if i < len {
            s[i]
        } else if flavor.is_sep(char_at(s, i - 1)) {
            break;
        } else {
            '/'
        };

        if flavor.is_sep(ch) {
            if last_slash == i as isize - 1 || dots == 1 {
                // empty segment or "."
            } else if dots == 2 {
                if res.len() < 2
                    || last_segment_length != 2
                    || res[res.len() - 1] != '.'
                    || res[res.len() - 2] != '.'
                {
                    if res.len() > 2 {
                        match last_index_of(&res, sep) {
                            Some(idx) => {
                                res.truncate(idx);
                                let inner = last_index_of(&res, sep).map(|v| v as isize).unwrap_or(-1);
                                last_segment_length = (res.len() as isize - 1 - inner) as usize;
                            }
                            None => {
                                res.clear();
                                last_segment_length = 0;
                            }
                        }
                        last_slash = i as isize;
                        dots = 0;
                        continue;
                    } else if !res.is_empty() {
                        res.clear();
                        last_segment_length = 0;
                        last_slash = i as isize;
                        dots = 0;
                        continue;
                    }
                }
                if allow_above_root {
                    if !res.is_empty() {
                        res.push(sep);
                    }
                    res.push('.');
                    res.push('.');
                    last_segment_length = 2;
                }
            } else {
                let from = (last_slash + 1) as usize;
                if !res.is_empty() {
                    res.push(sep);
                }
                res.extend_from_slice(&s[from..i]);
                last_segment_length = i - from;
            }
            last_slash = i as isize;
            dots = 0;
        } else if ch == '.' && dots != -1 {
            dots += 1;
        } else {
            dots = -1;
        }
    }
    res.into_iter().collect()
}

fn char_at(s: &[char], i: usize) -> char {
    s.get(i).copied().unwrap_or('\0')
}

fn last_index_of(s: &[char], target: char) -> Option<usize> {
    s.iter().rposition(|&c| c == target)
}

/// Collect a `&str` into a `Vec<char>` (the working representation).
pub fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}
