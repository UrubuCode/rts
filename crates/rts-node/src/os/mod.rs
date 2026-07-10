//! `node:os` — real system info, Node-accurate names/values.
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

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

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

fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Registers the `node:os` surface into the engine Registry. No `.alias(...)`
/// — `node:os` resolves as its own canonical key, distinct from the existing
/// `rts:os` namespace (RTS-flavored names/semantics), so it must not alias
/// onto it.
pub fn register(e: &mut Engine) {
    e.ns("node:os")
        .doc(
            "Real system info, Node-accurate names/values (node:os). EOL/devNull/ \
             constants (properties), cpus()/networkInterfaces()/userInfo() \
             (objects/arrays), and hostname()/release()/version()/totalmem()/ \
             freemem()/uptime()/loadavg() (need syscalls beyond std) are deferred.",
        )
        .member(pure_func(
            "platform",
            "__RTS_FN_NODE_OS_PLATFORM",
            sig!(=> Handle),
            "platform(): string",
            "Canonical Node platform name: 'win32', 'darwin', 'linux', 'freebsd', ...",
            __RTS_FN_NODE_OS_PLATFORM as *const u8,
        ))
        .member(pure_func(
            "arch",
            "__RTS_FN_NODE_OS_ARCH",
            sig!(=> Handle),
            "arch(): string",
            "Canonical Node CPU architecture name: 'x64', 'arm64', 'ia32', ...",
            __RTS_FN_NODE_OS_ARCH as *const u8,
        ))
        .member(pure_func(
            "type",
            "__RTS_FN_NODE_OS_TYPE",
            sig!(=> Handle),
            "type(): string",
            "Kernel name Node uses: 'Windows_NT', 'Darwin', 'Linux', ...",
            __RTS_FN_NODE_OS_TYPE as *const u8,
        ))
        .member(pure_func(
            "endianness",
            "__RTS_FN_NODE_OS_ENDIANNESS",
            sig!(=> Handle),
            "endianness(): string",
            "Native byte order of the target: 'LE' or 'BE'.",
            __RTS_FN_NODE_OS_ENDIANNESS as *const u8,
        ))
        .member(pure_func(
            "homedir",
            "__RTS_FN_NODE_OS_HOMEDIR",
            sig!(=> Handle),
            "homedir(): string",
            "Real user home directory. Empty string if unresolvable.",
            __RTS_FN_NODE_OS_HOMEDIR as *const u8,
        ))
        .member(pure_func(
            "tmpdir",
            "__RTS_FN_NODE_OS_TMPDIR",
            sig!(=> Handle),
            "tmpdir(): string",
            "System temporary directory, no trailing path separator.",
            __RTS_FN_NODE_OS_TMPDIR as *const u8,
        ))
        .member(pure_func(
            "availableParallelism",
            "__RTS_FN_NODE_OS_AVAILABLE_PARALLELISM",
            sig!(=> F64),
            "availableParallelism(): number",
            "Logical CPUs available to this process (fallback 1 on query error).",
            __RTS_FN_NODE_OS_AVAILABLE_PARALLELISM as *const u8,
        ))
        .done();
}
