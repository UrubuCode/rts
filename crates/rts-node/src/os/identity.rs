//! node:os — identity / path string functions (all return a GC string handle).
//!
//! `platform`/`arch`/`type`/`endianness`/`machine` are compiled-in or derived;
//! `release`/`version`/`hostname` are real running-kernel syscalls (see
//! [`crate::os::sys`]); `homedir`/`tmpdir` follow Node's env-var precedence;
//! `EOL`/`devNull` are the per-platform literals (registered as `Constant`
//! properties). No fake values.

use super::sys;
use super::words::{env_or_empty, intern, node_arch, node_platform, node_type};

/// `os.platform()` — canonical Node platform name.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_PLATFORM() -> u64 {
    intern(node_platform())
}

/// `os.arch()` — canonical Node CPU architecture name.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_ARCH() -> u64 {
    intern(node_arch())
}

/// `os.type()` — `uname -s`-style OS name (real `sysname` on POSIX).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_TYPE() -> u64 {
    intern(&node_type())
}

/// `os.endianness()` — `"LE"` or `"BE"` for the target's native byte order.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_ENDIANNESS() -> u64 {
    let e: &str = if cfg!(target_endian = "little") {
        "LE"
    } else {
        "BE"
    };
    intern(e)
}

/// `os.machine()` — raw `uname -m`-style hardware identifier.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_MACHINE() -> u64 {
    intern(&sys::machine())
}

/// `os.release()` — OS/kernel release string (real syscall).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_RELEASE() -> u64 {
    intern(&sys::release())
}

/// `os.version()` — human-readable kernel/OS build identifier (real syscall).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_VERSION() -> u64 {
    intern(&sys::version())
}

/// `os.hostname()` — the OS hostname (real `gethostname`/`GetComputerNameExW`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_HOSTNAME() -> u64 {
    intern(&sys::hostname())
}

/// `os.homedir()` — user home directory. `USERPROFILE` (Windows) / `HOME`
/// (POSIX), with a passwd-database fallback on POSIX when `HOME` is unset.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_HOMEDIR() -> u64 {
    #[cfg(windows)]
    let home = env_or_empty("USERPROFILE");
    #[cfg(unix)]
    let home = {
        let h = env_or_empty("HOME");
        if h.is_empty() {
            super::userinfo::passwd_homedir().unwrap_or_default()
        } else {
            h
        }
    };
    #[cfg(not(any(unix, windows)))]
    let home = env_or_empty("HOME");
    intern(&home)
}

/// `os.tmpdir()` — temp directory with Node's exact env-var precedence and no
/// trailing separator. Windows: `TEMP` → `TMP` → `%SystemRoot%\temp`. POSIX:
/// `TMPDIR` → `TMP` → `TEMP` → `/tmp`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_TMPDIR() -> u64 {
    let raw = tmpdir_raw();
    let trimmed = strip_trailing_sep(&raw);
    intern(trimmed)
}

fn tmpdir_raw() -> String {
    #[cfg(windows)]
    {
        for key in ["TEMP", "TMP"] {
            let v = env_or_empty(key);
            if !v.is_empty() {
                return v;
            }
        }
        let sysroot = env_or_empty("SystemRoot");
        if !sysroot.is_empty() {
            return format!("{sysroot}\\temp");
        }
        "C:\\Windows\\temp".to_string()
    }
    #[cfg(not(windows))]
    {
        for key in ["TMPDIR", "TMP", "TEMP"] {
            let v = env_or_empty(key);
            if !v.is_empty() {
                return v;
            }
        }
        "/tmp".to_string()
    }
}

/// Strip a single trailing `/` or `\` (but never reduce a root like `/` or
/// `C:\` to empty), matching Node's `os.tmpdir()` behavior.
fn strip_trailing_sep(s: &str) -> &str {
    let trimmed = s.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        // Was all separators (e.g. "/") — keep the original root.
        s
    } else {
        trimmed
    }
}

/// `os.EOL` — line ending for the platform (`Constant` property getter).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_EOL() -> u64 {
    intern(if cfg!(windows) { "\r\n" } else { "\n" })
}

/// `os.devNull` — the null device path (`Constant` property getter).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_DEV_NULL() -> u64 {
    intern(if cfg!(windows) { "\\\\.\\nul" } else { "/dev/null" })
}
