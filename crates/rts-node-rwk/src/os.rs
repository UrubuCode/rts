//! `node:os` — facts about the machine a program runs on.
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
//! behavioural gain. `hostname`, `tmpdir`, `homedir`, `totalmem`, `freemem`,
//! `uptime` are read on EVERY call, matching Node: Node's own docs mark
//! `freemem`/`uptime` as live and there is no reason a host directory or a
//! hostname would be more stable inside one process than Node assumes.
//!
//! # Not implemented, by name
//!
//! `loadavg`, `networkInterfaces`, `userInfo`, `getPriority`/`setPriority` —
//! each needs a real syscall (`getloadavg`, `getifaddrs`/`GetAdaptersAddresses`,
//! `getpwuid_r`/`GetUserNameW`, `getpriority`/`SetPriorityClass`) that `std`
//! alone does not expose, and this crate has no dependency on `libc` or
//! `windows-sys` to reach for one — `Cargo.toml` is `lib.rs`'s to own, not
//! this file's. Each answers `undefined` rather than a plausible wrong value.
//!
//! `constants` — the `signals`/`errno`/`dlopen`/`priority`/`libuv` tables.
//! `priority` and `libuv` are fixed, Node-level values with no OS dependency
//! and could be added without new machinery; `signals` and `errno` need
//! platform-correct numeric values (POSIX errno numbers differ across Linux/
//! macOS/Windows, and are not literals this crate can safely invent without
//! `libc`) — refused as a whole rather than half-built, since a `constants`
//! object missing `errno`/`signals` while claiming the name is the "plausible
//! wrong value" this module's own doctrine refuses elsewhere.
//!
//! # `cpus()` — a named divergence
//!
//! Node's `os.cpus()` reports a per-core model string and clock speed read
//! from the OS (`/proc/cpuinfo` on Linux, a registry key on Windows). `std`
//! alone has neither — `std::thread::available_parallelism` is the one count
//! it exposes, with no name and no frequency behind it. This answers one
//! entry per available core, each with `model: "unknown"` and `speed: 0`,
//! rather than one entry (which would misreport a multi-core machine as
//! having one CPU) or a fabricated model name.
//!
//! # `availableParallelism()`
//!
//! Node's version is cgroup/affinity-aware, which needs platform-specific
//! reads this crate has no dependency for (see `constants`, above); this
//! answers `std::thread::available_parallelism()` directly — the same count
//! `cpus()` uses — which is correct off a container and an overcount inside
//! one with a CPU quota. Named here rather than silently claimed exact.
//!
//! # `totalmem()` / `freemem()` — a named divergence
//!
//! `std` has no cross-platform memory query. On Linux, `/proc/meminfo` is a
//! text file `std::fs` can read like any other, so this parses it there. On
//! every other target (this crate is built and tested on Windows) there is no
//! `std`-only path to the figure, and this answers `0.0` rather than a number
//! that looks measured and is not. A caller dividing by `totalmem()` on a
//! non-Linux target will divide by zero — the loud failure the honesty floor
//! prefers over a silent wrong one.
//!
//! # `uptime()` — a named divergence
//!
//! Node answers the OS's uptime — since boot, across every process. `std` has
//! no boot-time API, so this answers the uptime of THIS PROCESS, measured
//! from the first call into this namespace, which is the closest available
//! number and is not the number Node's name promises.

use rts_core_rwk::entry::{Context, Provided};
use std::sync::OnceLock;

/// The namespace `node:os` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("platform", platform),
        ("arch", arch),
        ("type", type_),
        ("tmpdir", tmpdir),
        ("homedir", homedir),
        ("hostname", hostname),
        ("cpus", cpus),
        ("totalmem", totalmem),
        ("freemem", freemem),
        ("uptime", uptime),
        ("endianness", endianness),
        ("machine", machine),
        ("release", release),
        ("version", version),
        ("availableParallelism", available_parallelism),
    ];
    let namespace = rts_core_rwk::entry::make_namespace(context, members);
    let eol = rts_core_rwk::entry::make_string(context, EOL);
    rts_core_rwk::entry::put_member(context, namespace, "EOL", eol);
    let dev_null = rts_core_rwk::entry::make_string(context, DEV_NULL);
    rts_core_rwk::entry::put_member(context, namespace, "devNull", dev_null);
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
    string(arch_name())
}

fn arch_name() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "ia32",
        "aarch64" => "arm64",
        other => other,
    }
}

/// `os.type()` — the OS kernel name, in Node's spelling.
extern "C" fn type_(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let name = match std::env::consts::OS {
        "windows" => "Windows_NT",
        "macos" => "Darwin",
        "linux" => "Linux",
        other => other,
    };
    string(name)
}

/// `os.machine()` — the raw architecture name, distinct from `os.arch()`'s
/// Node-normalized one (Node's `arch()` says `"x64"`; `machine()` says
/// `"x86_64"`).
///
/// A named divergence: real Node calls `uname -m`, which can differ from the
/// Rust *compile* target on an emulated/translated host; this reports the
/// compiled target, matching `os.arch()`'s own compiled-in nature (see
/// `docs/reference/node/os.md` §4 on `arch`/`platform` being compiled-in).
extern "C" fn machine(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let name = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "x86" => "i686",
        "aarch64" => "aarch64",
        other => other,
    };
    string(name)
}

/// `os.endianness()`.
extern "C" fn endianness(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    #[cfg(target_endian = "big")]
    let name = "BE";
    #[cfg(target_endian = "little")]
    let name = "LE";
    string(name)
}

/// `os.release()` — a named divergence.
///
/// Node calls `uname -r`/reads the Windows build number, both real syscalls
/// this crate has no dependency to reach (see the module doc). This answers
/// the compiled OS-architecture pair instead of the running kernel's
/// version — a non-empty, truthful-about-the-BUILD string rather than an
/// invented kernel number.
extern "C" fn release(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string(&format!("{}-{}", std::env::consts::OS, arch_name()))
}

/// `os.version()` — same divergence as `release()`, same reason.
extern "C" fn version(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string(&format!("{}-{}", std::env::consts::OS, arch_name()))
}

/// `os.tmpdir()`.
extern "C" fn tmpdir(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string(&std::env::temp_dir().to_string_lossy())
}

/// `os.homedir()`.
///
/// Read from the environment variable the platform actually sets —
/// `USERPROFILE` on Windows, `HOME` elsewhere — rather than
/// `std::env::home_dir`, which `std` itself documents as wrong on Windows in
/// the presence of certain non-Unicode variables.
extern "C" fn homedir(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    #[cfg(windows)]
    let held = std::env::var("USERPROFILE");
    #[cfg(not(windows))]
    let held = std::env::var("HOME");
    string(&held.unwrap_or_default())
}

/// `os.hostname()` — a named divergence.
///
/// Node calls `gethostname(2)`. `std` alone has no such call, so this reads
/// the environment variable a shell typically sets instead —
/// `COMPUTERNAME` on Windows, `HOSTNAME` on Unix — which is unset in enough
/// environments (most Unix shells never export it) that this answers `""`
/// there rather than a name it does not have.
extern "C" fn hostname(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    #[cfg(windows)]
    let held = std::env::var("COMPUTERNAME");
    #[cfg(not(windows))]
    let held = std::env::var("HOSTNAME");
    string(&held.unwrap_or_default())
}

/// `os.cpus()` — see the module doc for what `model` and `speed` are here.
extern "C" fn cpus(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let count = parallelism();
    let entries = (0..count)
        .map(|_| {
            rts_core_rwk::entry::with_runtime(|context| {
                let object = rts_core_rwk::entry::make_object(context);
                let model = rts_core_rwk::entry::make_string(context, "unknown");
                rts_core_rwk::entry::put_member(context, object, "model", model);
                rts_core_rwk::entry::put_member(context, object, "speed", number(0.0));
                object
            })
        })
        .collect();
    rts_core_rwk::entry::make_array(entries)
}

/// `os.availableParallelism()` — see the module doc for the divergence from
/// Node's cgroup/affinity-aware count.
extern "C" fn available_parallelism(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    number(parallelism() as f64)
}

fn parallelism() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

/// `os.totalmem()` — see the module doc for the non-Linux divergence.
extern "C" fn totalmem(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    number(meminfo_field("MemTotal:").unwrap_or(0.0))
}

/// `os.freemem()` — see the module doc for the non-Linux divergence.
extern "C" fn freemem(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    number(meminfo_field("MemAvailable:").unwrap_or(0.0))
}

/// One `key: N kB` line out of `/proc/meminfo`, in bytes.
///
/// `None` off Linux, and `None` if the file is unreadable — both answer the
/// same `0.0` at the call site, because neither is a number this host has.
#[cfg(target_os = "linux")]
fn meminfo_field(key: &str) -> Option<f64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = text.lines().find(|line| line.starts_with(key))?;
    let kib: f64 = line
        .trim_start_matches(key)
        .trim()
        .trim_end_matches(" kB")
        .parse()
        .ok()?;
    Some(kib * 1024.0)
}

#[cfg(not(target_os = "linux"))]
fn meminfo_field(_key: &str) -> Option<f64> {
    None
}

/// `os.uptime()` — process uptime; see the module doc for the divergence
/// from Node's system uptime.
extern "C" fn uptime(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    static STARTED: OnceLock<std::time::Instant> = OnceLock::new();
    let started = STARTED.get_or_init(std::time::Instant::now);
    number(started.elapsed().as_secs_f64())
}

/// A string value.
fn string(text: &str) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, text))
}

/// A number value.
///
/// `rts_core_rwk::entry::modules` has no numeric-value wrapper the way it has
/// [`rts_core_rwk::entry::boolean_value`] — this mirrors exactly what that one
/// does, over `make_number`, which the host-facing surface grew when the first
/// are the same public `Value` type this crate's own helpers are built on.
fn number(value: f64) -> u64 {
    rts_core_rwk::entry::make_number(value)
}
