//! node:os — base extern "C" symbol implementations (the sync surface).
//!
//! Native rts-node implementation (no rts-std mirror; the `rts:os` namespace
//! keeps its own RTS-flavored names/semantics — see
//! `crates/rts-std/src/os/mod.rs`). This slice covers the pure, synchronous,
//! flat-value surface Node exposes as top-level functions:
//!
//! - `platform()` / `arch()` / `type()` / `endianness()` — derived from
//!   `std::env::consts::{OS,ARCH}` + `cfg!(target_endian)`, remapped to the
//!   exact strings Node uses (`"win32"`/`"darwin"`, `"x64"`/`"arm64"`/`"ia32"`,
//!   `"Windows_NT"`/`"Darwin"`/`"Linux"`, `"LE"`/`"BE"`).
//! - `homedir()` — `USERPROFILE` (Windows) / `HOME` (Unix), same resolution
//!   `rts-std/src/os/mod.rs::__RTS_FN_NS_OS_HOME_DIR` uses.
//! - `tmpdir()` — `std::env::temp_dir()` with any trailing path separator
//!   stripped (Node's `os.tmpdir()` never returns a trailing slash).
//! - `availableParallelism()` — `std::thread::available_parallelism()`.
//!
//! **Deferred** (need object/array/property machinery this flat-function slice
//! doesn't have, or a syscall/crate this scope doesn't pull in — no fake
//! values are substituted for any of these):
//! - `EOL`, `devNull`, `constants` — these are *properties*, not functions
//!   (`os.EOL`, not `os.EOL()`); this pure-function slice has no property
//!   surface. (`rts:os.eol()` already exists as a function for the RTS side.)
//! - `cpus()` — returns an array of per-core objects (model/speed/times);
//!   needs object-array marshalling, not in scope here.
//! - `networkInterfaces()` — returns a keyed object of interface-name →
//!   array-of-address-objects; needs the same object machinery, plus real
//!   NIC enumeration (no portable std API).
//! - `userInfo()` — returns an object (username/uid/gid/shell/homedir); needs
//!   object marshalling.
//! - `hostname()`, `release()`, `version()` — need real OS syscalls
//!   (`gethostname`, `uname`) not available through `std` alone; no crate for
//!   this is in scope, and a hardcoded/fake value would violate the honesty
//!   rule, so these are left unimplemented rather than faked.
//! - `totalmem()`, `freemem()` — need a sysinfo-style syscall
//!   (`GlobalMemoryStatusEx` / `/proc/meminfo` / `sysctl`) not available
//!   through `std` alone; not in scope, no fake numbers substituted.
//! - `uptime()` — needs a syscall (`GetTickCount64` / `/proc/uptime` /
//!   `sysctl(KERN_BOOTTIME)`); same reasoning.
//! - `loadavg()` — Unix-only concept (returns `[0, 0, 0]` on Windows in Node
//!   itself); needs `getloadavg`/`/proc/loadavg`, not in scope.
//!
//! ABI mirrors the pure-namespace shape used across RTS: no-arg functions
//! return a GC string handle (`intern`) or a native number; symbols follow the
//! rts-node convention `__RTS_FN_NODE_OS_*`.

use rts_engine::heap::shapes::{alloc_shaped_object, null_word, string_word};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Interns a Rust string as a GC string handle (the ABI `Handle` return).
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

fn env_or_empty(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

/// Node's `os.platform()` name for `std::env::consts::OS`: `"windows"` ->
/// `"win32"`, `"macos"` -> `"darwin"`, everything else passes through
/// unchanged (`"linux"`, `"freebsd"`, `"openbsd"`, `"android"`, `"ios"`, ...).
fn node_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    }
}

/// Node's `os.arch()` name for `std::env::consts::ARCH`.
fn node_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        "arm" => "arm",
        "powerpc64" => "ppc64",
        other => other,
    }
}

/// Node's `os.type()` — `"Windows_NT"` / `"Darwin"` / `"Linux"`, else a
/// best-effort capitalized `std::env::consts::OS` (no `uname`/syscall
/// available through `std` alone for the exotic targets).
fn node_type() -> String {
    if cfg!(target_os = "windows") {
        "Windows_NT".to_string()
    } else if cfg!(target_os = "macos") {
        "Darwin".to_string()
    } else if cfg!(target_os = "linux") {
        "Linux".to_string()
    } else {
        let os = std::env::consts::OS;
        let mut chars = os.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }
}

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

/// `os.type()` — kernel-name-shaped identifier Node uses.
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

/// `os.homedir()` — real user home directory (`USERPROFILE` on Windows,
/// `HOME` elsewhere). Empty string if unresolvable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_HOMEDIR() -> u64 {
    #[cfg(target_os = "windows")]
    let home = env_or_empty("USERPROFILE");
    #[cfg(not(target_os = "windows"))]
    let home = env_or_empty("HOME");
    intern(&home)
}

/// `os.tmpdir()` — system temp directory with any trailing path separator
/// stripped (Node never returns one).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_TMPDIR() -> u64 {
    let path = std::env::temp_dir();
    let s = path.to_string_lossy();
    let trimmed = s.trim_end_matches(['/', '\\']);
    intern(trimmed)
}

/// `os.availableParallelism()` — number of logical CPUs available to this
/// process, per `std::thread::available_parallelism()`. Falls back to `1` on
/// error (matches Node's own fallback behavior when the OS query fails).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_AVAILABLE_PARALLELISM() -> f64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(1.0)
}

/// `os.userInfo()` — an object `{ uid, gid, username, homedir, shell }`.
/// Built by REUSING the engine object primitive `alloc_shaped_object` (no
/// duplicated object machinery). On Windows `uid`/`gid` are `-1` and `shell`
/// is `null` (matching Node); on Unix `shell` is `$SHELL` and `uid`/`gid` are
/// `-1` for now (real `getuid`/`getgid` needs a syscall crate — deferred, not
/// faked to a wrong value). `username`/`homedir` are the real env values.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_USER_INFO() -> u64 {
    let username = if cfg!(windows) {
        env_or_empty("USERNAME")
    } else {
        env_or_empty("USER")
    };
    let homedir = if cfg!(windows) {
        env_or_empty("USERPROFILE")
    } else {
        env_or_empty("HOME")
    };
    let neg_one = (-1.0f64).to_bits();
    let shell_word = if cfg!(windows) {
        null_word()
    } else {
        string_word(env_or_empty("SHELL").as_bytes())
    };
    let keys: &[&str] = &["uid", "gid", "username", "homedir", "shell"];
    let values: [i64; 5] = [
        neg_one as i64,
        neg_one as i64,
        string_word(username.as_bytes()) as i64,
        string_word(homedir.as_bytes()) as i64,
        shell_word as i64,
    ];
    alloc_shaped_object(keys, &values)
}
