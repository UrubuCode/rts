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
//!
//! **Deferred** (need object/array/stream/event machinery this flat-function
//! slice doesn't have — NOT stubbed with fake values):
//! - `argv` / `execArgv` / `env` — arrays/objects (need Registry array/object
//!   marshalling, not a flat scalar return).
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
//!
//! NOTE: `process` is also a Node *global* (no import required) — this module
//! is only the `node:process` **module-import** surface; global-object wiring,
//! if any, is a separate concern outside this pure-function slice.
//!
//! ABI mirrors the pure-namespace shape used across RTS: `Str` args arrive as
//! `(ptr, len)` and are rebuilt via `from_abi` (`None` on null / invalid
//! UTF-8); string results are interned to GC string handles. Symbols follow
//! the rts-node convention `__RTS_FN_NODE_PROCESS_*`.

use rts_engine::abi::str_abi::from_abi;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

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
