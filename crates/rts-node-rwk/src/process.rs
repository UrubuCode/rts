//! `node:process` — the running program, as an object.
//!
//! # Property vs function
//!
//! `platform`, `arch`, `version`, `versions`, `pid`, `argv`, `env` are
//! PROPERTIES here, not functions — `process.platform`, never
//! `process.platform()`. That is the opposite convention from [`super::os`]'s
//! `os.platform()`, and the two are deliberately not unified: Node itself
//! disagrees between the two modules, and a program written against Node
//! reads `process.platform` as a value.
//!
//! # Read once
//!
//! `argv`, `env`, `platform`, `arch`, `version`, `versions`, `pid` are all
//! read ONCE, at namespace construction, and become fixed values on the
//! namespace object. Node re-reads none of these during a run either —
//! `process.argv` and `process.env` are ordinary mutable properties in Node
//! only because everything reachable from `process` is a plain object there,
//! not because Node consults the OS again on each read. `cwd()` is the one
//! member that must NOT be captured this way, because a program can
//! `chdir` — Node has no `chdir` exposed on `process` in a browser-hosted
//! sense but does via `process.chdir` elsewhere in Node; this crate does not
//! implement `chdir` (see below), but `cwd()` stays a live call regardless,
//! since answering a directory read at startup under the name `cwd()` would
//! be a function that lies about being one.
//!
//! # `version` / `versions` — a named divergence
//!
//! There is no Node version behind this engine — this is not Node. `version`
//! and `versions.node` both answer this crate's own Cargo package version,
//! prefixed `v` the way Node's is, so a program's `console.log(process.version)`
//! prints something version-shaped rather than nothing. A program that
//! branches on a specific Node version will read the wrong one; that is the
//! divergence, named here rather than met with a fabricated Node number.
//!
//! # `exit(code)`
//!
//! Calls `std::process::exit`, which does not return, does not run Rust
//! destructors, and does not let the calling test harness observe the rest of
//! its own test — a harness invoking a program that calls `process.exit()`
//! takes the WHOLE HOST PROCESS down with it, not just the interpreted
//! program. That is what Node does too — `process.exit()` ends the OS
//! process — and reproducing anything softer here would be the divergence,
//! not the faithful behaviour.
//!
//! # Not implemented, by name
//!
//! `stdout`/`stderr`/`stdin`, `on` (no event emitter here), `nextTick`,
//! `chdir`, `kill`, `memoryUsage`, `cpuUsage`, `hrtime()` (the callable form;
//! only the module doc below explains why even `hrtime.bigint()` is out),
//! `execPath`, `execArgv`, `title`, `umask`, `getuid`/`getgid` and the rest of
//! the POSIX credential surface.
//!
//! # `hrtime.bigint()` — refused, by name
//!
//! Not implemented. `rts_core_rwk::entry` (the modules API this crate is
//! restricted to) exposes no way to construct a `BigInt` value from outside
//! the runtime crate — [`rts_core_rwk::entry::bigint_new`] is not re-exported
//! through the `modules` surface this module is built on — so there is no
//! honest value to answer with. `hrtime` itself is therefore not registered,
//! rather than registered as an object with only a `bigint` member missing.

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:process` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[("cwd", cwd), ("exit", exit)];
    let namespace = rts_core_rwk::entry::make_namespace(context, members);

    let argv = argv_value(context);
    rts_core_rwk::entry::put_member(context, namespace, "argv", argv);

    let env = env_value(context);
    rts_core_rwk::entry::put_member(context, namespace, "env", env);

    let platform_name = match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    };
    let platform = rts_core_rwk::entry::make_string(context, platform_name);
    rts_core_rwk::entry::put_member(context, namespace, "platform", platform);

    let arch_name = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "ia32",
        "aarch64" => "arm64",
        other => other,
    };
    let arch = rts_core_rwk::entry::make_string(context, arch_name);
    rts_core_rwk::entry::put_member(context, namespace, "arch", arch);

    let version_text = format!("v{}", env!("CARGO_PKG_VERSION"));
    let version = rts_core_rwk::entry::make_string(context, &version_text);
    rts_core_rwk::entry::put_member(context, namespace, "version", version);

    let versions = rts_core_rwk::entry::make_object(context);
    let node_version = rts_core_rwk::entry::make_string(context, &version_text);
    rts_core_rwk::entry::put_member(context, versions, "node", node_version);
    rts_core_rwk::entry::put_member(context, namespace, "versions", versions);

    let pid = rts_core_rwk::entry::make_number(f64::from(std::process::id()));
    rts_core_rwk::entry::put_member(context, namespace, "pid", pid);

    namespace
}

/// `process.argv` — element 0 the executable, element 1 the script.
///
/// Node's `argv[0]` is the path to the `node` binary and `argv[1]` is the
/// script path; both are what a program actually indexes, so element 0 here
/// is `std::env::current_exe()` rather than whatever `argv[0]` the OS handed
/// the process (which on Unix can be a bare, unqualified name). Everything
/// from `std::env::args()` after the first is appended after — that first
/// entry is what a native process loader supplies as its own argv[0] and is
/// discarded in favour of the resolved executable path so it is not counted
/// twice.
fn argv_value(context: &mut Context) -> u64 {
    let exe = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut values = vec![rts_core_rwk::entry::make_string(context, &exe)];
    for arg in std::env::args().skip(1) {
        values.push(rts_core_rwk::entry::make_string(context, &arg));
    }
    rts_core_rwk::entry::make_array_in(context, values)
}

/// `process.env` — a snapshot, taken once.
///
/// Node's `process.env` is a live-looking object backed by the real
/// environment, and a write to it changes what a child process inherits.
/// This crate has no such write-through — the `modules` API builds a plain
/// object — so this is a read-once SNAPSHOT: a program that reads
/// `process.env.FOO` sees what the host had at startup, and a program that
/// writes to `process.env.FOO` changes only its own copy. Re-reading the OS
/// environment on every property access would need a live accessor this
/// crate does not expose here, and a snapshot that silently stopped tracking
/// writes would be worse than one that says so.
fn env_value(context: &mut Context) -> u64 {
    let object = rts_core_rwk::entry::make_object(context);
    for (key, value) in std::env::vars() {
        let held = rts_core_rwk::entry::make_string(context, &value);
        rts_core_rwk::entry::put_member(context, object, &key, held);
    }
    object
}

/// `process.cwd()`.
extern "C" fn cwd(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let text = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, &text))
}

/// `process.exit(code)` — ends the OS process. See the module doc.
///
/// The exit code is read BEFORE calling `std::process::exit`, and nothing
/// here holds the runtime borrow across that call — there is no user code
/// left to run afterward for the borrow discipline to protect.
extern "C" fn exit(_e: u64, _this: u64, code: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = rts_core_rwk::entry::undefined_value();
    let status = match code == absent {
        true => 0,
        false => rts_core_rwk::value::Value(code).as_f64().unwrap_or(0.0) as i32,
    };
    std::process::exit(status);
}
