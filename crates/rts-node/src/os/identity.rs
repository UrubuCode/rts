//! node:os — identity / path string functions (all return a GC string handle).
//!
//! `platform`/`arch`/`type`/`endianness`/`machine` are compiled-in or derived;
//! `release`/`version`/`hostname` are real running-kernel syscalls (see
//! [`crate::os::sys`]); `homedir`/`tmpdir` follow Node's env-var precedence;
//! `EOL`/`devNull` are the per-platform literals (registered as `Constant`
//! properties). No fake values.

use rts_engine::abi::ty::Handle;

use super::sys;
use super::words::{env_or_empty, intern, node_arch, node_platform, node_type};

/// `os.platform()` — canonical Node platform name.
#[rtse::function(module = "node:os", value = "platform", pure)]
fn platform() -> String {
    node_platform().to_string()
}

/// `os.arch()` — canonical Node CPU architecture name.
#[rtse::function(module = "node:os", value = "arch", pure)]
fn arch() -> String {
    node_arch().to_string()
}

/// `os.type()` — `uname -s`-style OS name (real `sysname` on POSIX).
#[rtse::function(module = "node:os", value = "type", pure)]
fn os_type() -> String {
    node_type().to_string()
}

/// `os.endianness()` — `"LE"` or `"BE"` for the target's native byte order.
#[rtse::function(module = "node:os", value = "endianness", pure)]
fn endianness() -> String {
    let e: &str = if cfg!(target_endian = "little") {
        "LE"
    } else {
        "BE"
    };
    e.to_string()
}

/// `os.machine()` — raw `uname -m`-style hardware identifier.
#[rtse::function(module = "node:os", value = "machine", pure)]
fn machine() -> String {
    sys::machine().to_string()
}

/// `os.release()` — OS/kernel release string (real syscall).
#[rtse::function(module = "node:os", value = "release", pure)]
fn release() -> String {
    sys::release().to_string()
}

/// `os.version()` — human-readable kernel/OS build identifier (real syscall).
#[rtse::function(module = "node:os", value = "version", pure)]
fn version() -> String {
    sys::version().to_string()
}

/// `os.hostname()` — the OS hostname (real `gethostname`/`GetComputerNameExW`).
#[rtse::function(module = "node:os", value = "hostname")]
fn hostname() -> String {
    sys::hostname().to_string()
}

/// `os.homedir()` — user home directory. `USERPROFILE` (Windows) / `HOME`
/// (POSIX), with a passwd-database fallback on POSIX when `HOME` is unset.
#[rtse::function(module = "node:os", value = "homedir", pure)]
fn homedir() -> String {
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
    home.to_string()
}

/// `os.tmpdir()` — temp directory with Node's exact env-var precedence and no
/// trailing separator. Windows: `TEMP` → `TMP` → `%SystemRoot%\temp`. POSIX:
/// `TMPDIR` → `TMP` → `TEMP` → `/tmp`.
#[rtse::function(module = "node:os", value = "tmpdir", pure)]
fn tmpdir() -> String {
    let raw = tmpdir_raw();
    let trimmed = strip_trailing_sep(&raw);
    trimmed.to_string()
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
