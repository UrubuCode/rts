//! `node:wasi` — against `docs/reference/node/wasi.md`.
//!
//! # Reuse-check
//!
//! `rts-cranelift` answers nothing here — running foreign WebAssembly
//! bytecode is not a machine capability this crate's own compiler pipeline
//! has any path into (checked `src/shape/`, `src/sched/`, `src/abi/`; none
//! of them decode `.wasm`). `rts-core::entry::Context` holds no Wasm
//! table either. The nearest existing shape in THIS crate is `fs/dir.rs`'s
//! `TABLE: Mutex<HashMap<u64, Cursor>>` — a process-global registry keyed by
//! a native id, holding state a JS instance carries as a number — and this
//! module follows the same shape for the same reason: a `WASI` instance
//! needs one identity a later `start`/`initialize`/`getImportObject` call
//! must find again, and `rts-node` cannot reach `super::class_support`
//! (a private submodule of `rts-core`), so a table keyed by a number is
//! the only place to keep it.
//!
//! # The one divergence this module states up front: no `WebAssembly` global
//!
//! Real Node's `wasi.start(instance)` takes a `WebAssembly.Instance` —
//! created through the JS-standard `WebAssembly.compile`/`instantiate`,
//! which V8 implements and `node:wasi` merely consumes. This engine has no
//! `WebAssembly` global (searched: it is not in `architecture.md`'s
//! primordial/ABI inventory, and nothing under `crates/` installs a name
//! `WebAssembly` on the global object). Building one is a second execution
//! engine's JS-facing surface — module/instance/memory/table objects,
//! `compile`/`instantiate` as async-shaped entry points — which is out of
//! scope for a `node:wasi` module and belongs, if ever, to its own spec (the
//! reference doc's own §5.6/§7 says the same).
//!
//! So `start`/`initialize` here take the WASM MODULE'S RAW BYTES (a
//! `Uint8Array`) directly, and this module owns compiling and running them
//! itself, through `wasmi`, rather than receiving an already-instantiated
//! object. **This is not `node:wasi`'s documented API shape** — it is the
//! honest shape available without a `WebAssembly` global underneath it, and
//! it is named here rather than presented as the real thing. A caller
//! porting Node code that calls `WebAssembly.instantiate(bytes,
//! wasi.getImportObject())` then `wasi.start(instance)` cannot use this
//! module unmodified — it must call `wasi.start(bytes)` instead.
//!
//! `wasi.wasiImport` / `wasi.getImportObject()` are still exposed, for
//! structural parity with code that reads them without calling into them,
//! but calling one of `wasiImport`'s functions directly from JS does
//! nothing useful: there is no `WebAssembly.Memory` for it to read or write,
//! because nothing here ever hands one to JS. The real host functions run
//! entirely inside `start`/`initialize`, wired straight into `wasmi`'s own
//! `Linker` (see [`host`]) — `wasiImport`'s entries are inert stand-ins, not
//! the functions actually called.
//!
//! # `preopens`: read and refused, not silently ignored
//!
//! [`WasiEnvEntry::preopens`] is populated from the constructor's `preopens`
//! option and never consulted again. No filesystem syscall in [`host`]
//! succeeds for ANY path — every `path_*`/`fd_*` filesystem call answers
//! `ENOSYS`/`ENOTCAPABLE` regardless of whether a preopen was configured.
//! Node's own docs are explicit that "an application given no preopens can
//! access no host directory at all" — the correct reading is that a
//! preopen is what GRANTS access, so recording one without wiring
//! `path_open`'s resolution through it would grant nothing while looking
//! like it might; refusing every filesystem call by name is the safe
//! reading and the one this module takes, per the caller's instruction.
//!
//! # `new WASI({ args, env })` used to abort the whole process — fixed 2026-09
//!
//! Not a JS exception, an actual Rust panic: `[RTS PANIC] RefCell already
//! borrowed`, from [`constructor`] reading `args`/`env`/`preopens` through
//! `entry::get_indexed`/`entry::own_keys` while ALREADY inside the
//! `entry::with_runtime` that read `version`/`stdin`/`stdout`/`stderr`. Both
//! are ambient entry points — they take the runtime borrow themselves — and
//! nesting one inside an open borrow is exactly the class
//! `docs/reference/node/STATUS.md`'s "rule every module here pays" names.
//! Found by `tests/claude-node-wasi-crash.test.ts`, isolating `args` and
//! `env` each alone; fixed by pulling those three fields' reads OUTSIDE the
//! constructor's borrow entirely — see [`read_string_array`]/
//! [`read_string_map`]'s own docs for why they take no `context` parameter
//! at all, which is the opposite correction from the usual one this codebase
//! makes for this class of bug.
//!
//! # Not implemented, by name
//!
//! - **`WebAssembly.Instance` as `start`/`initialize`'s argument** — see
//!   above; both take raw Wasm bytes instead.
//! - **Every filesystem syscall** (`path_open`, `path_filestat_get`,
//!   `path_create_directory`, `path_remove_directory`, `path_unlink_file`,
//!   `path_rename`, `path_readlink`, `path_symlink`, `fd_seek`, `fd_tell`,
//!   `fd_fdstat_get`/`_set_flags`, `fd_filestat_get`/`_set_size`/`_set_times`,
//!   `fd_readdir`, `fd_sync`, `fd_advise`, `fd_allocate`, `fd_prestat_get`,
//!   `fd_prestat_dir_name`) — `ENOSYS`/`ENOTCAPABLE`, always; see above.
//! - **`sock_accept`/`sock_recv`/`sock_send`/`sock_shutdown`** — `ENOSYS`,
//!   matching the stance most real WASI runtimes (reportedly including
//!   Node's own) take per the reference doc's §7.
//! - **`poll_oneoff`** — `ENOSYS`; a real implementation needs to multiplex
//!   over the fd table this module does not have (only stdin/stdout/stderr
//!   exist here).
//! - **`finalizeBindings`'s multithread story** — refused outright (see
//!   [`finalize_bindings`]); this module has no shared-memory/worker-thread
//!   wiring to give it meaning, and constructing one is `worker_threads`'
//!   job, not this module's.
//! - **`version: 'unstable'`'s `wasi_unstable` namespace** — the constructor
//!   accepts and records it, but [`host::run`] always builds the linker
//!   under `wasi_snapshot_preview1`; a module compiled expecting the older
//!   `wasi_unstable` import names fails to link. Preview1 is what every
//!   syscall in [`host`] implements.
//! - **Exact WASI errno-vs-Node mapping** — the numeric values in [`errno`]
//!   are the WASI preview1 spec's own assigned numbers, not independently
//!   checked against Node's C++ binding source; the reference doc's own §7
//!   flags this as unconfirmed.

mod errno;
mod host;
#[cfg(test)]
mod host_tests;

use rts_core::entry::{self, Context, Provided};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// One `new WASI(...)` call's fixed environment. Immutable after
/// construction — Node's own `WASIOptions` are constructor-only, nothing
/// mutates `args`/`env`/`preopens` afterward.
struct WasiEnvEntry {
    args: Vec<String>,
    env: Vec<(String, String)>,
    /// Read at construction, and never consulted again — see the module
    /// doc's "`preopens`: read and refused" section. Kept on the struct
    /// (rather than dropped at read time) so that fact is visible from the
    /// type rather than only from the doc comment.
    #[allow(dead_code)]
    preopens: Vec<(String, String)>,
    stdin_fd: i32,
    stdout_fd: i32,
    stderr_fd: i32,
    return_on_exit: bool,
    version: String,
    /// `start`/`initialize` are call-once per instance (Node's own rule);
    /// this is what enforces it.
    started: bool,
}

static TABLE: Mutex<Option<HashMap<u64, WasiEnvEntry>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, WasiEnvEntry>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

const METHODS: &[(&str, Provided)] = &[
    ("start", start),
    ("initialize", initialize),
    ("getImportObject", get_import_object),
    ("finalizeBindings", finalize_bindings),
];

/// The `node:wasi` namespace: `{ WASI }`, per the reference doc's Import
/// forms row — `WASI` is the module's only member.
///
/// # Why `WASI` is wired by hand instead of through [`entry::make_namespace`]'s
/// member list, and what installing it that way cost
///
/// `new WASI(options)` answered `undefined` for every options object,
/// unconditionally — while the bare call `WASI(options)` (no `new`) answered
/// the real instance. Traced rather than guessed at, into
/// `rts-core/src/entry/functions.rs::construct_inner`: `new` allocates the
/// object that becomes `this` by reading `callee.prototype`
/// (`allocate_for_target`), and when that property is absent it does not run
/// the constructor at all — it pops the pending `new.target` and answers
/// `undefined` directly. `entry::make_namespace`'s member list builds each
/// entry through `native::install`, which names it and hides it but never
/// writes a `.prototype` — correct for an ordinary namespace FUNCTION
/// (`fs.readFileSync.prototype` is `undefined` in Node too), wrong for a
/// namespace member that is a CLASS. `sqlite::namespace` and `vm::namespace`
/// already avoid this by building `DatabaseSync`/`Script` as a bare
/// `make_callable` with `.prototype` set by hand; this now does the same,
/// plus [`entry::declare_host_class`] for the `prototype.constructor`
/// back-link `stream::class_ctor`'s doc names as the second half of this same
/// class of gap (`new WASI(...).constructor.name` was `"Object"`, same as
/// every `node:stream` class before that fix).
pub fn namespace(context: &mut Context) -> u64 {
    let namespace = entry::make_namespace(context, &[]);
    let ctor = entry::make_callable(context, constructor);
    // The SAME object [`constructor`] links every instance to — `make_prototype`
    // is memoised by name, so building it again here finds it rather than
    // making a second one two instances could disagree about.
    let prototype = entry::make_prototype(context, "WASI", METHODS);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::declare_host_class(context, ctor, prototype, "WASI", 1);
    entry::put_member(context, namespace, "WASI", ctor);
    namespace
}

/// `new WASI(options)`.
extern "C" fn constructor(_e: u64, _this: u64, options: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // `version`/`stdin`/`stdout`/`stderr`/`returnOnExit` stay in ONE borrow:
    // `read_string`/`read_i32`/`read_bool` call only `entry::get_member`,
    // `entry::text_in`, `entry::undefined_in` (context-taking) and
    // `entry::number_of` (no borrow at all — see its own doc), so nesting
    // them here is safe.
    let (version, stdin_fd, stdout_fd, stderr_fd, return_on_exit) = entry::with_runtime(|context| {
        let version = read_string(context, options, "version").unwrap_or_default();
        // Node throws when `version` is missing/invalid. A native entry point
        // here cannot throw across the boundary any more usefully than an
        // `undefined` instance a caller's `.start()` then also answers
        // `undefined` from — so an invalid version is recorded as itself and
        // every syscall table build later refuses it (see `host::run`).
        let stdin_fd = read_i32(context, options, "stdin").unwrap_or(0);
        let stdout_fd = read_i32(context, options, "stdout").unwrap_or(1);
        let stderr_fd = read_i32(context, options, "stderr").unwrap_or(2);
        let return_on_exit = read_bool(context, options, "returnOnExit").unwrap_or(true);
        (version, stdin_fd, stdout_fd, stderr_fd, return_on_exit)
    });
    // `args`/`env`/`preopens` are read OUTSIDE any held borrow — each helper
    // opens and closes its own, see their docs. Reading these three inside
    // the block above is what panicked `[RTS PANIC] RefCell already
    // borrowed` on the two most ordinary `WASI` options — `args` and `env`,
    // and by the same code path `preopens` — before a `WASI` instance ever
    // existed. See tests/claude-node-wasi-crash.test.ts.
    let args = read_string_array(options, "args");
    let env = read_string_map(options, "env");
    let preopens = read_string_map(options, "preopens");

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| {
        table.insert(
            id,
            WasiEnvEntry {
                args,
                env,
                preopens,
                stdin_fd,
                stdout_fd,
                stderr_fd,
                return_on_exit,
                version,
                started: false,
            },
        );
    });

    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "WASI", METHODS);
        let instance = entry::make_instance(context, prototype);
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "__wasiId", id_value);
        let import_object = build_import_object(context, id);
        // `wasiImport` is the raw table `getImportObject()` wraps — same
        // object either way, matching Node's own
        // `result.wasi_snapshot_preview1 === wasi.wasiImport` contract,
        // stated inert in the module doc.
        let wasi_import = entry::get_member(context, import_object, wasi_namespace_key(&entry_version(id)));
        entry::put_member(context, instance, "wasiImport", wasi_import);
        instance
    })
}

fn entry_version(id: u64) -> String {
    with_table(|table| table.get(&id).map(|entry| entry.version.clone())).unwrap_or_default()
}

fn wasi_namespace_key(version: &str) -> &'static str {
    match version {
        "unstable" => "wasi_unstable",
        _ => "wasi_snapshot_preview1",
    }
}

/// Placeholder syscall names — see the module doc for why these are inert.
/// Names only, matching what a program iterating `Object.keys(wasiImport)`
/// would see; not the full ~46-entry preview1 set, since nothing here reads
/// them back by anything but presence.
const INERT_SYSCALL_NAMES: &[&str] = &[
    "args_get",
    "args_sizes_get",
    "environ_get",
    "environ_sizes_get",
    "clock_time_get",
    "random_get",
    "proc_exit",
    "sched_yield",
    "fd_write",
    "fd_read",
    "fd_close",
];

extern "C" fn inert_syscall(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}

/// Builds a FRESH `{ wasi_snapshot_preview1: { ...11 inert fns } }` (or the
/// `wasi_unstable` spelling). Called exactly once per `WASI` instance, from
/// [`constructor`] — [`get_import_object`] does NOT call this a second time;
/// see its own doc for why re-calling it would have been the wrong fix.
fn build_import_object(context: &mut Context, id: u64) -> u64 {
    let version = entry_version(id);
    let table = entry::make_object(context);
    // A plain OBJECT, not [`entry::make_namespace`]: that builder's members
    // go through `native::install`, which marks each one non-enumerable —
    // correct for a class's prototype methods, wrong here, where `wasiImport`
    // is a Node data object and `Object.keys(wasiImport)` is documented to
    // list every syscall name. `make_namespace` answered `[]` for exactly
    // that reason before this fixed it — confirmed against real Node, not
    // assumed from the shape of the bug.
    let syscalls = entry::make_object(context);
    for name in INERT_SYSCALL_NAMES {
        let callable = entry::make_callable(context, inert_syscall);
        entry::put_member(context, syscalls, name, callable);
    }
    entry::put_member(context, table, wasi_namespace_key(&version), syscalls);
    table
}

fn id_of(this: u64) -> Option<u64> {
    let value = entry::get_indexed(this, string_key("__wasiId"));
    entry::number_of(value).map(|value| value as u64)
}

fn string_key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

fn read_string(context: &mut Context, object: u64, key: &str) -> Option<String> {
    let value = entry::get_member(context, object, key);
    entry::text_in(context, value)
}

fn read_i32(context: &mut Context, object: u64, key: &str) -> Option<i32> {
    let value = entry::get_member(context, object, key);
    entry::number_of(value).map(|value| value as i32)
}

fn read_bool(context: &mut Context, object: u64, key: &str) -> Option<bool> {
    let absent = entry::undefined_in(context);
    let value = entry::get_member(context, object, key);
    if value == absent {
        return None;
    }
    Some(entry::number_of(value).map(|number| number != 0.0).unwrap_or(true))
}

/// A `string[]` option (`args`), or empty when absent.
///
/// # Why this does not take `context: &mut Context`
///
/// It cannot stay inside a caller's borrow and be correct: `entry::get_indexed`
/// is AMBIENT — it opens and closes the runtime borrow itself, on purpose, so
/// that a getter or a proxy trap the array might run does so with no borrow
/// held (its own doc says so). Calling it from inside a `with_runtime` this
/// function's caller already held is the nested borrow
/// `docs/reference/node/STATUS.md` names as fatal — before this fix it
/// aborted the process on `new WASI({ args: [...] })`, the same shape
/// `sqlite::database::option_raw` documents for a scalar option. So the one
/// context-taking step — reading `object[key]` itself — opens and closes its
/// own borrow, and every element after it is read the ambient way.
fn read_string_array(object: u64, key: &str) -> Vec<String> {
    let array = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let value = entry::get_member(context, object, key);
        (value != absent).then_some(value)
    });
    let Some(array) = array else {
        return Vec::new();
    };
    let length_value = entry::get_indexed(array, string_key("length"));
    let length = entry::number_of(length_value).map(|value| value as usize).unwrap_or(0);
    (0..length)
        .filter_map(|index| {
            let item = entry::get_indexed(array, entry::make_number(index as f64));
            entry::text_of(item)
        })
        .collect()
}

/// A `Record<string, string>` option (`env`, `preopens`), or empty when
/// absent. Does not take `context` for the same reason [`read_string_array`]
/// does not: `entry::own_keys` and `entry::get_indexed` are both ambient.
fn read_string_map(object: u64, key: &str) -> Vec<(String, String)> {
    let map = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let value = entry::get_member(context, object, key);
        (value != absent).then_some(value)
    });
    let Some(map) = map else {
        return Vec::new();
    };
    let names = entry::own_keys(map);
    let length_value = entry::get_indexed(names, string_key("length"));
    let length = entry::number_of(length_value).map(|value| value as usize).unwrap_or(0);
    (0..length)
        .filter_map(|index| {
            let name_value = entry::get_indexed(names, entry::make_number(index as f64));
            let name = entry::text_of(name_value)?;
            let value_value = entry::get_indexed(map, string_key(&name));
            let value = entry::text_of(value_value)?;
            Some((name, value))
        })
        .collect()
}

/// `wasi.start(wasmBytes)` — see the module doc: `wasmBytes` is a
/// `Uint8Array` of the module's raw bytes, not a `WebAssembly.Instance`.
/// Runs it as a WASI COMMAND (`_start`). Answers the exit code (a number)
/// when `returnOnExit` is set (the default); `undefined` on any failure
/// (bad bytes, no `_start`, already started, memory missing, …) — this
/// crate's natives cannot throw across the boundary any more than
/// [`constructor`] can.
extern "C" fn start(_e: u64, this: u64, bytes: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    run(this, bytes, host::Entry::Command)
}

/// `wasi.initialize(wasmBytes)` — the reactor counterpart of [`start`]:
/// runs `_initialize` if present, hands control back rather than to a
/// "main". Answers `undefined` always (Node's own `initialize` has no
/// return value).
extern "C" fn initialize(_e: u64, this: u64, bytes: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    run(this, bytes, host::Entry::Reactor)
}

fn run(this: u64, bytes: u64, entry_kind: host::Entry) -> u64 {
    let absent = entry::undefined_value();
    let Some(id) = id_of(this) else {
        return absent;
    };
    let Some(wasm_bytes) = entry::with_runtime(|context| entry::bytes_of(context, bytes)) else {
        return absent;
    };
    let already_started = with_table(|table| {
        table.get(&id).map(|entry| entry.started).unwrap_or(true)
    });
    if already_started {
        return absent; // call-once; a second call is refused rather than re-run.
    }
    let outcome = with_table(|table| {
        let Some(env) = table.get(&id) else {
            return None;
        };
        Some(host::run(env, &wasm_bytes, entry_kind))
    });
    with_table(|table| {
        if let Some(entry) = table.get_mut(&id) {
            entry.started = true;
        }
    });
    match outcome {
        Some(Ok(exit_code)) => match entry_kind {
            host::Entry::Reactor => absent,
            host::Entry::Command => {
                let return_on_exit = with_table(|table| table.get(&id).map(|entry| entry.return_on_exit).unwrap_or(true));
                match return_on_exit {
                    true => entry::make_number(exit_code as f64),
                    false => std::process::exit(exit_code),
                }
            }
        },
        _ => absent,
    }
}

/// `wasi.getImportObject()` — see the module doc: structurally shaped like
/// Node's (`{ wasi_snapshot_preview1: wasi.wasiImport }` or the `unstable`
/// equivalent), but there is no `WebAssembly.instantiate` in this engine
/// for it to be the second argument to, and its functions are inert.
///
/// # Why this reads `this.wasiImport` back rather than calling
/// [`build_import_object`] a second time
///
/// Node documents `wasi.getImportObject().wasi_snapshot_preview1 ===
/// wasi.wasiImport` — the SAME object, because Node's own implementation is
/// `{ wasi_snapshot_preview1: this.wasiImport }`, reading a value it stored
/// once. Calling `build_import_object` again here built a SECOND `syscalls`
/// object, with 11 freshly minted `inert_syscall` callables none of which
/// `===` the ones on `wasiImport` — the identity Node promises, broken.
/// The outer wrapper `{ wasi_snapshot_preview1: … }` is still built fresh
/// per call, matching Node: nothing documents THAT object's identity as
/// stable across two calls, only the inner one.
extern "C" fn get_import_object(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(id) = id_of(this) else {
        return absent;
    };
    entry::with_runtime(|context| {
        let wasi_import = entry::get_member(context, this, "wasiImport");
        let table = entry::make_object(context);
        entry::put_member(context, table, wasi_namespace_key(&entry_version(id)), wasi_import);
        table
    })
}

/// `wasi.finalizeBindings(...)` — refused; see the module doc's "Not
/// implemented, by name" section. Answers `undefined` rather than throwing,
/// the same stand-in every other refusal in this module uses.
extern "C" fn finalize_bindings(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}
