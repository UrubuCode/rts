//! node:path — the `extern "C"` entry points (per flavor). Each decodes its
//! string args, calls the pure algorithm ([`super::posix`]/[`super::win32`]/
//! [`super::classify`]), and interns the result. `join`/`resolve` are variadic
//! in Node; since native members resolve by exact arity, each is emitted as a
//! set of fixed-arity overloads (0..8) generated locally below.

pub mod posix;
pub mod win32;

use super::words::intern;

/// Decode an ABI `(ptr, len)` string arg (empty on null/invalid).
pub(crate) fn read_str<'a>(ptr: *const u8, len: i64) -> &'a str {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("")
}

/// The process current working directory (used by `resolve`).
pub(crate) fn process_cwd() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Win32 per-drive working directory: the hidden `=X:` environment variable
/// Windows maintains, falling back to `X:\` (and, off Windows, the process CWD
/// — no per-drive concept exists there).
pub(crate) fn drive_cwd(letter: char) -> String {
    #[cfg(windows)]
    {
        let key = format!("={}:", letter.to_ascii_uppercase());
        if let Ok(v) = std::env::var(&key) {
            if !v.is_empty() {
                return v;
            }
        }
        format!("{}:\\", letter.to_ascii_uppercase())
    }
    #[cfg(not(windows))]
    {
        let _ = letter;
        process_cwd()
    }
}

fn posix_join(parts: &[String]) -> String {
    super::posix::join(parts)
}
fn posix_resolve(parts: &[String]) -> String {
    super::posix::resolve(parts, &process_cwd())
}
fn win32_join(parts: &[String]) -> String {
    super::win32::join(parts)
}
fn win32_resolve(parts: &[String]) -> String {
    super::win32::resolve(parts, &process_cwd(), drive_cwd)
}

/// `path.<flavor>.sep` / `.delimiter` — `Constant` property getters.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_SEP() -> u64 {
    intern("/")
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_DELIMITER() -> u64 {
    intern(":")
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_SEP() -> u64 {
    intern("\\")
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_DELIMITER() -> u64 {
    intern(";")
}

/// Generate the fixed-arity overloads (0..8) of a variadic `parts`-taking fn.
macro_rules! arity_variants {
    ($impl:path; $( $name:ident => [ $( $p:ident $l:ident ),* ] ),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub extern "C" fn $name($( $p: *const u8, $l: i64 ),*) -> u64 {
                let parts: Vec<String> = vec![$( read_str($p, $l).to_string() ),*];
                intern(&$impl(&parts))
            }
        )*
    };
}

arity_variants!(posix_join;
    __RTS_FN_NODE_PATH_POSIX_JOIN0 => [],
    __RTS_FN_NODE_PATH_POSIX_JOIN1 => [a la],
    __RTS_FN_NODE_PATH_POSIX_JOIN2 => [a la, b lb],
    __RTS_FN_NODE_PATH_POSIX_JOIN3 => [a la, b lb, c lc],
    __RTS_FN_NODE_PATH_POSIX_JOIN4 => [a la, b lb, c lc, d ld],
    __RTS_FN_NODE_PATH_POSIX_JOIN5 => [a la, b lb, c lc, d ld, e le],
    __RTS_FN_NODE_PATH_POSIX_JOIN6 => [a la, b lb, c lc, d ld, e le, f lf],
    __RTS_FN_NODE_PATH_POSIX_JOIN7 => [a la, b lb, c lc, d ld, e le, f lf, g lg],
    __RTS_FN_NODE_PATH_POSIX_JOIN8 => [a la, b lb, c lc, d ld, e le, f lf, g lg, h lh],
);
arity_variants!(posix_resolve;
    __RTS_FN_NODE_PATH_POSIX_RESOLVE0 => [],
    __RTS_FN_NODE_PATH_POSIX_RESOLVE1 => [a la],
    __RTS_FN_NODE_PATH_POSIX_RESOLVE2 => [a la, b lb],
    __RTS_FN_NODE_PATH_POSIX_RESOLVE3 => [a la, b lb, c lc],
    __RTS_FN_NODE_PATH_POSIX_RESOLVE4 => [a la, b lb, c lc, d ld],
    __RTS_FN_NODE_PATH_POSIX_RESOLVE5 => [a la, b lb, c lc, d ld, e le],
    __RTS_FN_NODE_PATH_POSIX_RESOLVE6 => [a la, b lb, c lc, d ld, e le, f lf],
    __RTS_FN_NODE_PATH_POSIX_RESOLVE7 => [a la, b lb, c lc, d ld, e le, f lf, g lg],
    __RTS_FN_NODE_PATH_POSIX_RESOLVE8 => [a la, b lb, c lc, d ld, e le, f lf, g lg, h lh],
);
arity_variants!(win32_join;
    __RTS_FN_NODE_PATH_WIN32_JOIN0 => [],
    __RTS_FN_NODE_PATH_WIN32_JOIN1 => [a la],
    __RTS_FN_NODE_PATH_WIN32_JOIN2 => [a la, b lb],
    __RTS_FN_NODE_PATH_WIN32_JOIN3 => [a la, b lb, c lc],
    __RTS_FN_NODE_PATH_WIN32_JOIN4 => [a la, b lb, c lc, d ld],
    __RTS_FN_NODE_PATH_WIN32_JOIN5 => [a la, b lb, c lc, d ld, e le],
    __RTS_FN_NODE_PATH_WIN32_JOIN6 => [a la, b lb, c lc, d ld, e le, f lf],
    __RTS_FN_NODE_PATH_WIN32_JOIN7 => [a la, b lb, c lc, d ld, e le, f lf, g lg],
    __RTS_FN_NODE_PATH_WIN32_JOIN8 => [a la, b lb, c lc, d ld, e le, f lf, g lg, h lh],
);
arity_variants!(win32_resolve;
    __RTS_FN_NODE_PATH_WIN32_RESOLVE0 => [],
    __RTS_FN_NODE_PATH_WIN32_RESOLVE1 => [a la],
    __RTS_FN_NODE_PATH_WIN32_RESOLVE2 => [a la, b lb],
    __RTS_FN_NODE_PATH_WIN32_RESOLVE3 => [a la, b lb, c lc],
    __RTS_FN_NODE_PATH_WIN32_RESOLVE4 => [a la, b lb, c lc, d ld],
    __RTS_FN_NODE_PATH_WIN32_RESOLVE5 => [a la, b lb, c lc, d ld, e le],
    __RTS_FN_NODE_PATH_WIN32_RESOLVE6 => [a la, b lb, c lc, d ld, e le, f lf],
    __RTS_FN_NODE_PATH_WIN32_RESOLVE7 => [a la, b lb, c lc, d ld, e le, f lf, g lg],
    __RTS_FN_NODE_PATH_WIN32_RESOLVE8 => [a la, b lb, c lc, d ld, e le, f lf, g lg, h lh],
);
