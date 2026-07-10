//! node:process — base extern "C" symbol implementations (the sync surface).
//!
//! Native rts-node implementation (no rts-std mirror; the existing rts-std
//! `process` NS namespace has RTS-flavored semantics/values — this module
//! exposes the real, Node-named/mapped surface instead). Flat, synchronous
//! functions only:
//!
//! - `platform(): string` — Node's `process.platform` value, derived from
//!   `std::env::consts::OS` via the same `win32`/`darwin`/`linux`/... mapping
//!   Node's own platform detection uses (see [`node_platform`]).
//! - `arch(): string` — Node's `process.arch` value, derived from
//!   `std::env::consts::ARCH` via the same `x64`/`ia32`/`arm64`/... mapping
//!   (see [`node_arch`]).
//! - `pid(): number` — `std::process::id()`.
//! - `cwd(): string` — `std::env::current_dir()`.
//! - `chdir(dir: string): void` — `std::env::set_current_dir()`.
//! - `exit(code: number): void` — `std::process::exit()`.
//! - `uptime(): number` — fractional seconds of real monotonic time since this
//!   module's uptime origin, lazily established on first read (see
//!   [`ORIGIN`]; this module has no earlier process-start hook to
//!   anchor to, so "process start" here means "first `uptime()`/origin read",
//!   not the OS process creation time — documented approximation, not a fake
//!   value).
//! - `env(): object` — a snapshot object of every environment variable visible
//!   to this process (`std::env::vars_os()`, keys/values that aren't valid
//!   Unicode are silently skipped — fail-soft, matching the rest of this
//!   slice's error handling; never panics). **Shape difference from Node:**
//!   Node exposes `process.env` as a live, mutable object *property* on the
//!   global `process` object (reads/writes touch the real environment as they
//!   happen); this module has no property surface, so `env()` is a **function**
//!   returning a point-in-time snapshot object instead — mutating the returned
//!   object does not write back to the process environment.
//! - `argv(): string[]` — a snapshot array of every process argument
//!   (`std::env::args_os()`, entries that aren't valid Unicode are silently
//!   skipped). **Shape difference from Node:** Node exposes `process.argv` as
//!   an array *property* (`[execPath, scriptPath, ...userArgs]`); this module
//!   returns a plain snapshot array from `argv()` instead of a property, with
//!   whatever this process's actual `argv` is (no execPath/scriptPath
//!   convention is imposed — that shaping is a caller/harness concern).
//!
//! **Deferred** (need object/array/stream/event machinery this flat-function
//! slice doesn't have — NOT stubbed with fake values):
//! - `execArgv` — needs distinguishing interpreter flags from script args, a
//!   convention this flat process-args view doesn't carry.
//! - `ppid(): number` — Rust's `std` has **no portable parent-pid API on any
//!   target** (stable Rust exposes no `getppid`/equivalent without a
//!   platform-syscall crate like `libc`, which is not a dependency of this
//!   crate — see `Cargo.toml`); returning anything here without that syscall
//!   would be an invented value, so `ppid()` is deferred rather than faked.
//! - `versions` / `version` / `title` / `argv0` — object/string *properties*
//!   (not module functions) read on the `process` global object itself, which
//!   this pure `node:process` module slice does not model. `version` in
//!   particular is deliberately NOT added here as a function: RTS is not Node,
//!   so there is no genuine Node version to report, and returning ANY string
//!   would be an invented/fake value — exactly what the honesty floor forbids.
//! - `nextTick` — needs the microtask queue; `hrtime` / `hrtime.bigint` — need
//!   BigInt (not yet a value this ABI carries) and high-res timing semantics
//!   beyond a flat number.
//! - `stdout` / `stderr` / `stdin` — Stream objects (need the stream +
//!   event-emitter machinery, not flat functions).
//! - `on(event, listener)` (signals, `'exit'`, `'uncaughtException'`, ...) —
//!   needs EventEmitter-style callback registration.
//! - `memoryUsage()` / `cpuUsage()` / `resourceUsage()` — need a
//!   syscall/crate (`GetProcessMemoryInfo`, `getrusage`, ...) not available
//!   through `std` alone; not in scope, no fake numbers substituted.
//!
//! NOTE: `process` is also a Node *global* (no import required) — this module
//! is only the `node:process` **module-import** surface; global-object wiring,
//! if any, is a separate concern outside this pure-function slice.
//!
//! ABI mirrors the pure-namespace shape used across RTS: `Str` args arrive as
//! `(ptr, len)` and are rebuilt via `from_abi` (`None` on null / invalid
//! UTF-8); string results are interned to GC string handles; `object` results
//! are built via `alloc_shaped_object_owned` (runtime-computed key lists —
//! the env-var name set varies per process); `string[]` results are
//! `Entry::Vec` of string WORDs. Symbols follow the rts-node convention
//! `__RTS_FN_NODE_PROCESS_*`.

use rts_engine::abi::str_abi::from_abi;
use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::{alloc_shaped_object_owned, string_word};
use std::sync::OnceLock;
use std::time::Instant;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Origin instant for `process.uptime()`. Node measures uptime from the
/// process's actual start (before any JS runs); RTS has no earlier process
/// hook exposed to this module, so the origin is lazily set on the **first**
/// call to `uptime()` (or any other reader of `ORIGIN`) via
/// `get_or_init` — the first read of process uptime in a given run is `~0`,
/// and every subsequent read reports real elapsed time since that first read.
/// This is a documented approximation, not a fake/fixed value: the underlying
/// clock (`std::time::Instant`) is genuine monotonic time.
static ORIGIN: OnceLock<Instant> = OnceLock::new();

/// Interns a Rust string as a GC string handle (the ABI `Handle` return).
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Maps `std::env::consts::OS` to Node's `process.platform` naming. Node's
/// documented values are `'aix' | 'darwin' | 'freebsd' | 'linux' | 'openbsd' |
/// 'sunos' | 'win32'` (plus the experimental `'android'`); anything Rust's std
/// recognizes outside that set is passed through as-is (still the genuine OS
/// name — never an invented Node value).
fn node_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        "solaris" | "illumos" => "sunos",
        // linux, freebsd, openbsd, aix, android already match Node's naming.
        other => other,
    }
}

/// Maps `std::env::consts::ARCH` to Node's `process.arch` naming. Node's
/// documented values are `'arm' | 'arm64' | 'ia32' | 'loong64' | 'mips' |
/// 'mipsel' | 'ppc' | 'ppc64' | 'riscv64' | 's390' | 's390x' | 'x64'`; anything
/// Rust's std recognizes outside that set is passed through as-is.
fn node_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "ia32",
        "aarch64" => "arm64",
        "powerpc" => "ppc",
        "powerpc64" => "ppc64",
        "loongarch64" => "loong64",
        // arm, mips, mipsel, s390x, riscv64 already match Node's naming.
        other => other,
    }
}

/// `process.platform` — same mapping Node itself uses ('win32', 'darwin',
/// 'linux', 'freebsd', 'openbsd', 'sunos', 'aix', ...).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROCESS_PLATFORM() -> u64 {
    intern(node_platform())
}

/// `process.arch` — same mapping Node itself uses ('x64', 'ia32', 'arm64',
/// 'arm', 'ppc', 'ppc64', 's390x', 'riscv64', ...).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROCESS_ARCH() -> u64 {
    intern(node_arch())
}

/// `process.pid` — the current process id.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROCESS_PID() -> f64 {
    std::process::id() as f64
}

/// `process.cwd()` — the current working directory. Empty string when it
/// cannot be resolved (deleted cwd, permission error, ...); Node throws in
/// that case, but this flat ABI has no exception channel, so an empty string
/// is the honest sentinel (never a fabricated path).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROCESS_CWD() -> u64 {
    match std::env::current_dir() {
        Ok(path) => intern(&path.to_string_lossy()),
        Err(_) => intern(""),
    }
}

/// `process.chdir(dir)` — changes the current working directory via the real
/// `std::env::set_current_dir` syscall. Node throws (e.g. `ENOENT`) on
/// failure; this member's signature is `void` (no return channel), so a
/// failure (missing dir, permission error, invalid UTF-8 arg) is silently a
/// no-op rather than panicking — the underlying call is still genuine, it
/// just cannot report the error back through this ABI shape.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROCESS_CHDIR(dir_ptr: *const u8, dir_len: i64) {
    let Some(dir) = (unsafe { from_abi(dir_ptr, dir_len) }) else {
        return;
    };
    let _ = std::env::set_current_dir(dir);
}

/// `process.exit(code)` — terminates the process immediately via
/// `std::process::exit`. Never returns.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROCESS_EXIT(code: i64) {
    std::process::exit(code as i32)
}

/// `process.uptime()` — seconds elapsed (real monotonic time, fractional)
/// since this module's uptime origin was first established (see [`ORIGIN`]).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROCESS_UPTIME() -> f64 {
    let origin = ORIGIN.get_or_init(Instant::now);
    origin.elapsed().as_secs_f64()
}

/// `process.env()` — a snapshot **object** of every environment variable
/// visible to this process (real `std::env::vars_os()`). Node exposes
/// `process.env` as a live property; this module has no property surface, so
/// this is a function returning a point-in-time snapshot instead (documented
/// shape difference, not a semantic substitute). Keys/values that are not
/// valid Unicode are silently skipped (fail-soft — `std::env::vars()` would
/// panic on those, so `vars_os()` + a lossless `to_str()` check is used
/// instead; never fabricates a value for an unrepresentable env var).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROCESS_ENV() -> u64 {
    let mut keys: Vec<String> = Vec::new();
    let mut values: Vec<i64> = Vec::new();
    for (k, v) in std::env::vars_os() {
        let (Some(k), Some(v)) = (k.to_str(), v.to_str()) else {
            continue;
        };
        keys.push(k.to_string());
        values.push(string_word(v.as_bytes()) as i64);
    }
    alloc_shaped_object_owned(keys, &values)
}

/// `process.argv()` — a snapshot **array** of every process argument (real
/// `std::env::args_os()`). Node exposes `process.argv` as an array property
/// shaped `[execPath, scriptPath, ...userArgs]`; this module has no property
/// surface and imposes no such convention, so this returns the process's raw
/// argument list as-is (documented shape difference). Entries that are not
/// valid Unicode are silently skipped (fail-soft — `std::env::args()` would
/// panic on those; `args_os()` + a lossless `to_str()` check is used instead).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROCESS_ARGV() -> u64 {
    let items: Vec<i64> = std::env::args_os()
        .filter_map(|a| a.to_str().map(|s| string_word(s.as_bytes()) as i64))
        .collect();
    alloc_entry(Entry::Vec(Box::new(items)))
}
