//! `node:os` — facts about the machine a program runs on.
//!
//! # Reuse check, before anything here was written
//!
//! Searched `rts-cranelift` (`src/abi`, `src/types`, `src/probe`) for anything
//! that reports a host fact: nothing does, and correctly — the machine layer is
//! about the compiled machine, not the box it runs on. Searched
//! `rts-core-rwk/src/entry/modules.rs`, the whole host surface, for a numeric,
//! array or object builder before writing one: `make_object`, `make_array_in`,
//! `make_number`, `make_buffer`, `null_in` and `string_in` are all there and are
//! what this module builds every answer out of. Searched this crate for an
//! existing reader of the same facts: `process.rs` normalizes `platform`/`arch`
//! from the same two `std::env::consts` — deliberately NOT shared, because Node
//! itself keeps `process.platform` and `os.platform()` as separate surfaces and
//! a single helper would tie two modules' answers together for no user-visible
//! gain — and `fs/perms.rs`/`fs/statfs.rs` hand-declare `extern "system"` Win32
//! entries over no new dependency, which is the precedent every Win32 call in
//! this folder follows rather than reaching for a new crate.
//!
//! # Property vs function
//!
//! Every member here is a FUNCTION, even `platform()` and `arch()`, which
//! cannot change during a run. That is what Node does — `os.platform()` is
//! called, `process.platform` is read — and the two modules are deliberately
//! not made to agree with each other: matching Node's own inconsistency is
//! what lets a program written against Node run unmodified against this.
//!
//! # Read once or read every call
//!
//! `platform`, `arch`, `type`, `EOL`, `devNull`, `endianness`, `machine`
//! cannot change during a process's life, so there is no difference to
//! observe between computing them at namespace construction and computing
//! them on each call — this computes them on each call anyway, because
//! building a namespace with a mix of precomputed strings and closures over
//! precomputed strings is a second encoding of the same string for zero
//! behavioural gain. Everything else is read on EVERY call, matching Node:
//! Node's own docs mark `freemem`/`uptime` as live, and a network interface,
//! a hostname or a priority is no more stable inside one process.
//!
//! `os.constants` is the one exception, and it is a REQUIREMENT rather than an
//! optimisation: its tables are compile-time data, and a program comparing
//! `os.constants.errno === os.constants.errno` across two reads must find the
//! same object. Built once, at namespace construction.
//!
//! # Not implemented, by name
//!
//! - **`os.getPriority` / `os.setPriority` do not throw.** Node raises
//!   `SystemError` (`ESRCH` for an unknown pid, `EPERM`/`EACCES` without the
//!   privilege); a native here cannot throw across the call boundary — the same
//!   limitation `node:fs` states — so `getPriority` answers `undefined` and
//!   `setPriority` answers `false` where Node throws. A wrong PRIORITY is never
//!   invented for either.
//! - **`os.userInfo()` does not throw** either, for the same reason, and
//!   answers `undefined` where Node raises `SystemError` for a user with no
//!   resolvable name.
//! - **`os.cpus()` reports no per-core detail on a Unix that is not Linux.**
//!   Linux is read from `/proc/cpuinfo` and `/proc/stat`, Windows from the
//!   kernel's own performance table and the processor registry key; macOS needs
//!   `host_processor_info`, which is a Mach call this crate has no binding for,
//!   and it answers the EMPTY array Node documents as the failure result rather
//!   than an array of zeroed `times` a utilisation calculation would silently
//!   read as 0%.
//! - **`os.totalmem()` / `os.freemem()` / `os.uptime()` on a Unix that is not
//!   Linux** answer `0` for the same reason (`sysctl`/`host_statistics64`); on
//!   Windows and Linux all three are real.
//! - **MAC addresses in `os.networkInterfaces()` off Windows and Linux.** Linux
//!   publishes them under `/sys/class/net`, Windows reports them with the
//!   adapter; every other Unix needs the `AF_LINK` pseudo-entry walk, and until
//!   that exists those entries carry Node's own `"00:00:00:00:00:00"`
//!   unavailable placeholder.
//! - **`os.availableParallelism()` is not cgroup-aware.** It answers
//!   `std::thread::available_parallelism()`, which respects the CPU affinity
//!   mask on every target but not a container's CPU quota — correct off a
//!   container, an overcount inside one. Node parses `/sys/fs/cgroup/cpu.max`;
//!   that is the gap, and it is named rather than silently claimed exact.
//! - **`os.constants.signals`/`errno` omit what the host libc does not define**
//!   rather than zero-filling them, which is what Node does too: `SIGBREAK` is
//!   absent as a KEY on POSIX and the `WSA*` family is absent as keys off
//!   Windows. `os.constants.dlopen` is an empty object on Windows, which is
//!   parity — there is no `dlopen(3)` there.
//! - **`os.machine()` reports the COMPILED target on Windows**, which has no
//!   `uname -m`; on POSIX it is the real `utsname.machine`. An emulated or
//!   translated Windows host would report the target RTS was built for.

mod constants;
mod cpus;
mod machine;
mod netif;
mod user;

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:os` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("platform", platform),
        ("arch", arch),
        ("type", type_),
        ("tmpdir", tmpdir),
        ("homedir", homedir),
        ("hostname", hostname),
        ("cpus", cpus_of),
        ("totalmem", totalmem),
        ("freemem", freemem),
        ("uptime", uptime),
        ("loadavg", loadavg),
        ("endianness", endianness),
        ("machine", machine_name),
        ("release", release),
        ("version", version),
        ("networkInterfaces", network_interfaces),
        ("userInfo", user_info),
        ("getPriority", get_priority),
        ("setPriority", set_priority),
        ("availableParallelism", available_parallelism),
    ];
    let namespace = rts_core_rwk::entry::make_namespace(context, members);
    let eol = rts_core_rwk::entry::make_string(context, EOL);
    rts_core_rwk::entry::put_member(context, namespace, "EOL", eol);
    let dev_null = rts_core_rwk::entry::make_string(context, DEV_NULL);
    rts_core_rwk::entry::put_member(context, namespace, "devNull", dev_null);
    let table = constants::object(context);
    rts_core_rwk::entry::put_member(context, namespace, "constants", table);
    namespace
}

/// The line ending this platform's text tools expect.
#[cfg(windows)]
const EOL: &str = "\r\n";
#[cfg(not(windows))]
const EOL: &str = "\n";

/// `os.devNull` — the discard file, in this platform's spelling.
#[cfg(windows)]
const DEV_NULL: &str = "\\\\.\\nul";
#[cfg(not(windows))]
const DEV_NULL: &str = "/dev/null";

/// `os.platform()` — Node's spelling, not Rust's.
///
/// Rust's `std::env::consts::OS` disagrees with Node on two of the three
/// targets this crate runs on: `"windows"` where Node says `"win32"`, and
/// `"macos"` where Node says `"darwin"`. `"linux"` agrees already. Every other
/// Rust spelling (`"freebsd"`, `"openbsd"`, …) is passed through unchanged,
/// because Node uses the same ones there.
extern "C" fn platform(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let name = match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    };
    string(name)
}

/// `os.arch()` — Node's spelling, not Rust's.
///
/// `std::env::consts::ARCH` disagrees on the two architectures this matters
/// for: `"x86_64"` where Node says `"x64"`, and `"x86"` where Node says
/// `"ia32"`. `"aarch64"` becomes Node's `"arm64"`; anything else passes
/// through.
extern "C" fn arch(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let name = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "ia32",
        "aarch64" => "arm64",
        other => other,
    };
    string(name)
}

/// `os.type()` — the OS kernel name, in Node's spelling.
extern "C" fn type_(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string(machine::kind())
}

/// `os.machine()` — the raw architecture name, a distinct enum from
/// `os.arch()`'s Node-normalized one (`arch()` says `"x64"`; this says
/// `"x86_64"`).
extern "C" fn machine_name(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string(&machine::machine())
}

/// `os.endianness()`.
extern "C" fn endianness(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    #[cfg(target_endian = "big")]
    let name = "BE";
    #[cfg(target_endian = "little")]
    let name = "LE";
    string(name)
}

/// `os.release()` — the RUNNING kernel's version, not the compiled target's.
extern "C" fn release(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string(&machine::release())
}

/// `os.version()` — the human-readable OS build identifier.
extern "C" fn version(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string(&machine::version())
}

/// `os.tmpdir()` — Node's own variable precedence, not `std::env::temp_dir`'s.
///
/// The two differ: `std` consults `TMPDIR` only on Unix and falls back to
/// `GetTempPath` on Windows, which appends a trailing separator Node's
/// documented answer does not have. A program joining the result would produce
/// a doubled separator.
extern "C" fn tmpdir(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    #[cfg(windows)]
    let names: &[&str] = &["TEMP", "TMP"];
    #[cfg(not(windows))]
    let names: &[&str] = &["TMPDIR", "TMP", "TEMP"];
    let held = names
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()));
    #[cfg(windows)]
    let fallback = || {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_owned());
        format!("{root}\\temp")
    };
    #[cfg(not(windows))]
    let fallback = || "/tmp".to_owned();
    string(&trim_separator(&held.unwrap_or_else(fallback)))
}

/// A path with no trailing separator, unless the path IS the separator.
///
/// `"C:\\"` and `"/"` keep theirs: stripping those turns a root into a drive
/// letter or an empty string, which is a different path rather than a tidier
/// spelling of the same one.
fn trim_separator(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.is_empty() || trimmed.ends_with(':') {
        true => path.to_owned(),
        false => trimmed.to_owned(),
    }
}

/// `os.homedir()`.
extern "C" fn homedir(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string(&home_directory())
}

/// The current user's home directory.
///
/// Read from the environment variable the platform actually sets —
/// `USERPROFILE` on Windows, `HOME` elsewhere — rather than
/// `std::env::home_dir`, which `std` itself documents as wrong on Windows in
/// the presence of certain non-Unicode variables. Falling back to the password
/// database off Windows is what makes it answer inside a daemon, where nothing
/// exported `HOME`.
pub(super) fn home_directory() -> String {
    #[cfg(windows)]
    {
        if let Ok(held) = std::env::var("USERPROFILE") {
            return held;
        }
        let drive = std::env::var("HOMEDRIVE").unwrap_or_default();
        let path = std::env::var("HOMEPATH").unwrap_or_default();
        return format!("{drive}{path}");
    }
    #[cfg(not(windows))]
    {
        match std::env::var("HOME") {
            Ok(held) if !held.is_empty() => held,
            _ => user::current().map(|held| held.homedir).unwrap_or_default(),
        }
    }
}

/// `os.hostname()` — the OS's own name for the machine.
extern "C" fn hostname(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string(&machine::hostname().unwrap_or_default())
}

/// `os.cpus()`.
extern "C" fn cpus_of(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let held = cpus::cpus();
    rts_core_rwk::entry::with_runtime(|context| cpus::value(context, held))
}

/// `os.availableParallelism()`.
extern "C" fn available_parallelism(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::make_number(cpus::parallelism() as f64)
}

/// `os.totalmem()`.
extern "C" fn totalmem(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::make_number(machine::memory().map_or(0.0, |(total, _)| total))
}

/// `os.freemem()`.
extern "C" fn freemem(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::make_number(machine::memory().map_or(0.0, |(_, free)| free))
}

/// `os.uptime()` — the SYSTEM's uptime, in seconds.
extern "C" fn uptime(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::make_number(machine::uptime().unwrap_or(0.0))
}

/// `os.loadavg()`.
extern "C" fn loadavg(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let held = machine::loadavg();
    rts_core_rwk::entry::with_runtime(|context| {
        let values = held.iter().map(|held| rts_core_rwk::entry::make_number(*held)).collect();
        rts_core_rwk::entry::make_array_in(context, values)
    })
}

/// `os.networkInterfaces()`.
extern "C" fn network_interfaces(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let held = netif::interfaces();
    rts_core_rwk::entry::with_runtime(|context| netif::value(context, held))
}

/// `os.userInfo([options])`.
extern "C" fn user_info(_e: u64, _this: u64, options: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| {
        // `string_in`, not `text_in`: this asks WHAT the encoding is, and a
        // coercion answers `"[object Object]"` for an object and `"42"` for a
        // number — the "coercion used as a type test" defect this repository
        // has already been bitten by three times.
        let encoding = rts_core_rwk::entry::get_member(context, options, "encoding");
        let wants_buffer = rts_core_rwk::entry::string_in(context, encoding)
            .is_some_and(|held| held == "buffer");
        user::value(context, wants_buffer)
    })
}

/// `os.getPriority([pid])`.
extern "C" fn get_priority(_e: u64, _this: u64, pid: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let pid = rts_core_rwk::entry::number_of(pid).unwrap_or(0.0).max(0.0) as u32;
    match user::priority_of(pid) {
        Some(held) => rts_core_rwk::entry::make_number(held),
        None => rts_core_rwk::entry::undefined_value(),
    }
}

/// `os.setPriority([pid, ]priority)` — answers whether it took.
///
/// # Why a boolean where Node answers `undefined`
///
/// Because Node's `undefined` there means "it worked, and it would have thrown
/// otherwise", and this cannot throw. An `undefined` from both outcomes would
/// be a silent no-op indistinguishable from success, which is the defect class
/// this crate refuses; a boolean is a stated divergence a caller can act on.
extern "C" fn set_priority(_e: u64, _this: u64, first: u64, second: u64, _a2: u64, _a3: u64) -> u64 {
    // The one-argument form puts the PRIORITY in the pid slot, which is the
    // same overload shift `fs::options_and_listener` resolves for its family —
    // reused in spirit, not in code, because the discriminator there is
    // "is it callable" and here it is "is there a second number at all".
    let (pid, priority) = match rts_core_rwk::entry::number_of(second) {
        Some(priority) => (rts_core_rwk::entry::number_of(first).unwrap_or(0.0), priority),
        None => (0.0, rts_core_rwk::entry::number_of(first).unwrap_or(0.0)),
    };
    rts_core_rwk::entry::boolean_value(user::set_priority(pid.max(0.0) as u32, priority))
}

/// A string value.
fn string(text: &str) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, text))
}
