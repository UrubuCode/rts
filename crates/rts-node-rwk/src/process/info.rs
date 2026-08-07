//! What the process IS: its arguments, its environment, and the identity of
//! the engine running it.
//!
//! Everything here is read once, at namespace construction — see the module
//! doc of [`super`] for why that is faithful rather than a shortcut, and for
//! what `env` being a snapshot costs.

use rts_core_rwk::entry;

/// Puts every fixed property on the namespace.
pub(super) fn install(context: &mut entry::Context, namespace: u64) {
    let exe = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();

    let argv = argv_value(context, &exe);
    entry::put_member(context, namespace, "argv", argv);

    // `argv0` is the RAW, unprocessed `argv[0]` the OS handed the process —
    // distinct from `argv[0]` above, which is `argv0` resolved to an
    // absolute path the way Node's own `argv[0]` is.
    let argv0_raw = std::env::args().next().unwrap_or_default();
    let argv0 = entry::make_string(context, &argv0_raw);
    entry::put_member(context, namespace, "argv0", argv0);

    let exec_path = entry::make_string(context, &exe);
    entry::put_member(context, namespace, "execPath", exec_path);

    // Node's `execArgv` holds the flags between the executable and the script.
    // This engine has no such vocabulary — every argument after the executable
    // is the program's — so empty is the true answer here rather than an
    // unfilled one, and it is an ARRAY because `process.execArgv.length` is how
    // a program asks.
    let exec_argv = entry::make_array_in(context, Vec::new());
    entry::put_member(context, namespace, "execArgv", exec_argv);

    let env = env_value(context);
    entry::put_member(context, namespace, "env", env);

    let platform_name = match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    };
    let platform = entry::make_string(context, platform_name);
    entry::put_member(context, namespace, "platform", platform);

    let arch_name = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "ia32",
        "aarch64" => "arm64",
        other => other,
    };
    let arch = entry::make_string(context, arch_name);
    entry::put_member(context, namespace, "arch", arch);

    identity(context, namespace);
    ids(context, namespace);

    // `exitCode` starts `undefined`, which is both Node's default and the value
    // that lets `exit()` tell "never set" from "set to 0". It is an ordinary
    // writable property: a program assigns to it, and `process.exit()` with no
    // argument reads it back. What it does NOT do is survive to a natural end
    // of the program — see the refusal list in [`super`].
    let unset = entry::undefined_in(context);
    entry::put_member(context, namespace, "exitCode", unset);
}

/// `version`, `versions`, `release`, `features` — who is running the program.
fn identity(context: &mut entry::Context, namespace: u64) {
    let version_text = format!("v{}", env!("CARGO_PKG_VERSION"));
    let version = entry::make_string(context, &version_text);
    entry::put_member(context, namespace, "version", version);

    // Two keys and not Node's twenty. `v8`, `uv`, `llhttp`, `openssl` and the
    // rest name components this engine does not contain, and a version string
    // for a component that is absent is a fabricated answer a `semver` gate
    // would act on.
    let versions = entry::make_object(context);
    let node_version = entry::make_string(context, &version_text);
    entry::put_member(context, versions, "node", node_version);
    let own = entry::make_string(context, env!("CARGO_PKG_VERSION"));
    entry::put_member(context, versions, "rts", own);
    entry::put_member(context, namespace, "versions", versions);

    // `name` only: `sourceUrl`/`headersUrl`/`libUrl` point at a download server
    // for a Node build that does not exist for this engine.
    let release = entry::make_object(context);
    let name = entry::make_string(context, "rts");
    entry::put_member(context, release, "name", name);
    entry::put_member(context, namespace, "release", release);

    // Only the flags that are facts about THIS build. `inspector`,
    // `cached_builtins`, `require_module` and the deprecated-but-frozen family
    // would each be a guess about a subsystem that is not here.
    let features = entry::make_object(context);
    let debug = entry::boolean_value(cfg!(debug_assertions));
    entry::put_member(context, features, "debug", debug);
    let tls = entry::boolean_value(true);
    entry::put_member(context, features, "tls", tls);
    // `"transform"` and not `"strip"`: this engine COMPILES TypeScript rather
    // than erasing its types and handing JavaScript to someone else, which is
    // exactly the distinction Node's two values draw.
    let typescript = entry::make_string(context, "transform");
    entry::put_member(context, features, "typescript", typescript);
    entry::put_member(context, namespace, "features", features);
}

/// `pid`, `ppid`, `title`.
fn ids(context: &mut entry::Context, namespace: u64) {
    let pid = entry::make_number(f64::from(std::process::id()));
    entry::put_member(context, namespace, "pid", pid);

    // POSIX only. Windows has a parent pid but reaching it means
    // `NtQueryInformationProcess` or a toolhelp snapshot, neither of which is
    // in `std` and neither of which may be added from a module file — so the
    // property is ABSENT there rather than answering someone else's number.
    #[cfg(unix)]
    {
        let parent = unsafe { libc::getppid() };
        let ppid = entry::make_number(f64::from(parent));
        entry::put_member(context, namespace, "ppid", ppid);
    }

    // The executable's own file name, which is what `ps` shows for a process
    // that never renamed itself. Assigning to this property does not reach the
    // OS; the module doc names that.
    let title_text = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_stem().map(|stem| stem.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "rts".to_owned());
    let title = entry::make_string(context, &title_text);
    entry::put_member(context, namespace, "title", title);
}

/// `process.argv` — element 0 the executable, element 1 the script.
///
/// Node's `argv[0]` is the path to the `node` binary and `argv[1]` is the
/// script path; both are what a program actually indexes, so element 0 here
/// is `std::env::current_exe()` rather than whatever `argv[0]` the OS handed
/// the process (which on Unix can be a bare, unqualified name; that raw value
/// is `process.argv0`, above). Everything from `std::env::args()` after the
/// first is appended after — that first entry is what a native process
/// loader supplies as its own argv[0] and is discarded in favour of the
/// resolved executable path so it is not counted twice.
fn argv_value(context: &mut entry::Context, exe: &str) -> u64 {
    let mut values = vec![entry::make_string(context, exe)];
    for arg in std::env::args().skip(1) {
        values.push(entry::make_string(context, &arg));
    }
    entry::make_array_in(context, values)
}

/// `process.env` — a snapshot, taken once.
///
/// Node's `process.env` is a live-looking object backed by the real
/// environment, and a write to it changes what a child process inherits.
/// Making that work needs a property whose read and write are FUNCTIONS, and
/// the host surface's `define_getter`/`define_setter` take a key number minted
/// from the compiler's own registry, which nothing outside `rts-core-rwk` can
/// mint — so a live `env` is not merely unwritten here, it is unreachable from
/// a module crate. A snapshot that silently stopped tracking writes would be
/// worse than one that says so, and the module doc says so.
fn env_value(context: &mut entry::Context) -> u64 {
    let object = entry::make_object(context);
    for (key, value) in std::env::vars() {
        let held = entry::make_string(context, &value);
        entry::put_member(context, object, &key, held);
    }
    object
}

/// `process.getBuiltinModule(id)` — the module object a specifier names,
/// without going through an import.
///
/// Asked of the runtime's own specifier table rather than of a second list of
/// names: `node:module`'s `builtinModules` already records what that costs when
/// the two disagree. One divergence, named: a specifier a compiled program
/// itself published (`"./a.ts"`) answers its namespace here, where Node would
/// answer `undefined` — the table does not distinguish who registered a name.
pub(super) extern "C" fn get_builtin_module(
    _e: u64,
    _this: u64,
    id: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::with_runtime(|context| {
        // `string_in`, not `text_of`: this asks WHAT the argument is. A number
        // coerced to `"5"` would be looked up as a specifier.
        match entry::string_in(context, id) {
            Some(text) => entry::module_at_name(context, &text),
            None => entry::undefined_in(context),
        }
    })
}

/// `process.loadEnvFile(path?)` — reads a `KEY=VALUE` file into `process.env`.
///
/// Into the SNAPSHOT object, which is the whole of what `env` is here: a
/// program reads `process.env.KEY` back and finds it, and a child process
/// spawned afterwards does not, for the same reason a plain assignment to
/// `process.env` does not reach it.
///
/// Node throws for a missing file. A native in this crate cannot throw (see
/// `assert.rs`'s module doc), so the failure is reported on stderr and the call
/// answers `undefined` — visible rather than silent, which is the most this
/// boundary allows.
pub(super) extern "C" fn load_env_file(
    _e: u64,
    _this: u64,
    path: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let absent = entry::undefined_value();
    let named = entry::with_runtime(|context| entry::string_in(context, path));
    let file = named.unwrap_or_else(|| ".env".to_owned());
    let source = match std::fs::read_to_string(&file) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("rts: process.loadEnvFile({file}): {error}");
            return absent;
        }
    };
    let environment = entry::with_runtime(|context| {
        entry::get_member(context, super::object(), "env")
    });
    for (key, value) in parse_env(&source) {
        entry::with_runtime(|context| {
            let held = entry::make_string(context, &value);
            entry::put_member(context, environment, &key, held);
        });
    }
    absent
}

/// `KEY=VALUE` lines, with `#` comments and one optional layer of quotes.
///
/// Deliberately not a dotenv implementation: no interpolation, no `export`
/// prefix, no multi-line values. What it does support is stated here so a
/// program author can tell whether their file is inside it.
fn parse_env(source: &str) -> Vec<(String, String)> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let value = value.trim();
            let unquoted = value
                .strip_prefix('"')
                .and_then(|rest| rest.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|rest| rest.strip_suffix('\'')))
                .unwrap_or(value);
            Some((key.trim().to_owned(), unquoted.to_owned()))
        })
        .collect()
}
